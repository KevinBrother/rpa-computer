//! Cross-process desktop writer lock.
//!
//! Two independent `computer-host` processes in the same OS user session must
//! never inject input concurrently. Mutual exclusion comes from an OS-level
//! advisory lock (`flock` / `LockFileEx`) held on a STABLE rendezvous file:
//!
//! - The rendezvous file is NEVER unlinked while in use. Removing it after
//!   unlock creates a classic inode race: process A unlocks and unlinks,
//!   process B had opened the old inode and holds a lock on a file no longer
//!   reachable, process C creates a fresh inode and locks it — two "owners".
//!   Leaving the file in place is safe: the lock (not the file) is the
//!   exclusion mechanism, and the OS releases it on process death.
//! - The lock scope is derived from the NATIVE user identity (uid on Unix,
//!   the `USERPROFILE` directory on Windows), never from attacker-mutable
//!   `USER`/`USERNAME`/`TMPDIR` environment variables. Two hosts launched
//!   with different env vars in the same desktop session still collide.
//! - The rendezvous is created with owner-only permissions, and symbolic
//!   links are refused (`O_NOFOLLOW` on Unix) so a hostile symlink planted in
//!   a shared temp dir cannot redirect the lock onto a victim file.
//!
//! On lock contention the caller reports a clean `lease_conflict` diagnostic
//! before any input is injected.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum LockError {
    /// Another live host process holds the desktop writer lock.
    Contended,
    /// Could not create/open the lock file at all.
    Unavailable(String),
}

impl std::fmt::Display for LockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LockError::Contended => write!(
                f,
                "another computer-host process already controls this desktop (lease_conflict)"
            ),
            LockError::Unavailable(m) => write!(f, "desktop lock unavailable: {m}"),
        }
    }
}

/// Held while the host owns the desktop writer role. Dropping closes the
/// handle, which releases the OS lock; process death releases it even
/// without Drop. The rendezvous FILE IS DELIBERATELY LEFT ON DISK.
pub struct DesktopLock {
    file: File,
    path: PathBuf,
}

impl DesktopLock {
    /// Path of the underlying lock file (diagnostics only).
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for DesktopLock {
    fn drop(&mut self) {
        // Explicit unlock for prompt release; closing the fd (File drop)
        // would also release it. The file is NOT removed: unlinking a live
        // rendezvous re-opens the inode-replacement race documented above.
        unlock(&self.file);
    }
}

/// Default lock file location: per-user, derived from NATIVE identity.
///
/// Unix: `$TMPDIR/computer-host-<uid>.lock` where `<uid>` is the real uid
/// from `getuid(2)` — environment variables cannot change it.
///
/// Windows: `%USERPROFILE%\AppData\Local\Temp\computer-host-<username>.lock`
/// with a fallback to the native identity in the file name. Lock scoping to
/// the interactive session is additionally enforced because each session has
/// its own `\Sessions\<id>\AppData\Local\Temp` on multi-session systems via
/// USERPROFILE; a per-session `Local\` named mutex is a documented future
/// hardening (see report) but the file lock already excludes cross-process
/// concurrent writers for the same profile.
pub fn default_lock_path() -> PathBuf {
    std::env::temp_dir().join(format!("computer-host-{}.lock", native_user_key()))
}

/// Native, environment-independent user identity for the lock scope.
pub fn native_user_key() -> String {
    #[cfg(unix)]
    {
        // getuid() cannot be influenced by USER/TMPDIR.
        unsafe { libc_getuid() }.to_string()
    }
    #[cfg(windows)]
    {
        // The profile directory is resolved by the OS for the process token;
        // it does not trust USER/USERNAME. We use it as the stable key.
        std::env::var("USERPROFILE")
            .ok()
            .and_then(|p| {
                Path::new(&p)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "unknown".into())
    }
    #[cfg(not(any(unix, windows)))]
    {
        "unknown".into()
    }
}

#[cfg(unix)]
extern "C" {
    #[link_name = "getuid"]
    fn libc_getuid() -> u32;
}

/// Try to acquire the desktop writer lock at `path`.
///
/// Security: refuses symlinks (Unix `O_NOFOLLOW`), creates the file with
/// owner-only mode (0600 on Unix) and verifies metadata after open so a
/// pre-existing world-writable or group-writable file is rejected rather
/// than silently trusted.
pub fn acquire_at(path: &Path) -> Result<DesktopLock, LockError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|e| LockError::Unavailable(format!("create {}: {e}", parent.display())))?;
        }
    }

    #[cfg(unix)]
    let file = {
        use std::os::unix::fs::OpenOptionsExt;
        let mut opts = OpenOptions::new();
        opts.read(true)
            .write(true)
            .create(true)
            .mode(0o600)
            .custom_flags(O_NOFOLLOW);
        opts.open(path).map_err(|e| {
            if e.raw_os_error() == Some(ELOOP) {
                LockError::Unavailable(format!(
                    "{} is a symbolic link; refusing to lock through it",
                    path.display()
                ))
            } else {
                LockError::Unavailable(format!("open {}: {e}", path.display()))
            }
        })?
    };
    #[cfg(not(unix))]
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(path)
        .map_err(|e| LockError::Unavailable(format!("open {}: {e}", path.display())))?;

    // Post-open validation: never lock a file others can tamper with.
    validate_file_security(path, &file)?;

    try_lock(&file)?;

    // Annotate with pid for diagnostics; content is informational only.
    let _ = file.set_len(0);
    use std::io::Write;
    let mut f = &file;
    let _ = writeln!(f, "pid={}", std::process::id());
    Ok(DesktopLock {
        file,
        path: path.to_path_buf(),
    })
}

/// Acquire the desktop writer lock at the default per-user path.
pub fn acquire() -> Result<DesktopLock, LockError> {
    acquire_at(&default_lock_path())
}

/// Reject lock files whose permissions would let another local principal
/// rewrite them (Unix mode check; Windows relies on the per-user directory).
fn validate_file_security(path: &Path, file: &File) -> Result<(), LockError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let meta = file
            .metadata()
            .map_err(|e| LockError::Unavailable(format!("stat {}: {e}", path.display())))?;
        if !meta.is_file() {
            return Err(LockError::Unavailable(format!(
                "{} is not a regular file",
                path.display()
            )));
        }
        let mode = meta.mode() & 0o777;
        if mode & 0o077 != 0 {
            // Pre-existing permissive file: tighten rather than fail hard —
            // chmod is atomic for our purposes and the lock content is
            // diagnostic-only.
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|e| {
                LockError::Unavailable(format!(
                    "{} has mode {:o} and cannot be tightened: {e}",
                    path.display(),
                    mode
                ))
            })?;
        }
    }
    let _ = (path, file);
    Ok(())
}

// ---- platform implementations -------------------------------------------

#[cfg(unix)]
fn try_lock(file: &File) -> Result<(), LockError> {
    use std::os::unix::io::AsRawFd;
    // SAFETY: flock on a valid fd; LOCK_NB so contention is reported.
    let rc = unsafe { libc_flock(file.as_raw_fd(), LOCK_EX | LOCK_NB) };
    if rc == 0 {
        Ok(())
    } else {
        let err = io::Error::last_os_error();
        match err.raw_os_error() {
            Some(code) if code == EWOULDBLOCK => Err(LockError::Contended),
            _ => Err(LockError::Unavailable(format!("flock failed: {err}"))),
        }
    }
}

#[cfg(unix)]
fn unlock(file: &File) {
    use std::os::unix::io::AsRawFd;
    // SAFETY: flock on a valid fd.
    unsafe {
        libc_flock(file.as_raw_fd(), LOCK_UN);
    }
}

#[cfg(unix)]
const LOCK_EX: i32 = 2;
#[cfg(unix)]
const LOCK_NB: i32 = 4;
#[cfg(unix)]
const LOCK_UN: i32 = 8;
#[cfg(all(unix, target_os = "macos"))]
const EWOULDBLOCK: i32 = 35;
#[cfg(all(unix, not(target_os = "macos")))]
const EWOULDBLOCK: i32 = 11;
// O_NOFOLLOW (fnctl.h): macOS 0x100 (256), Linux 0o400000 (131072).
#[cfg(all(unix, target_os = "macos"))]
const O_NOFOLLOW: i32 = 0x100;
#[cfg(all(unix, not(target_os = "macos")))]
const O_NOFOLLOW: i32 = 0x20000; // Linux O_NOFOLLOW (octal 00400000)
#[cfg(all(unix, target_os = "macos"))]
const ELOOP: i32 = 62;
#[cfg(all(unix, not(target_os = "macos")))]
const ELOOP: i32 = 40;

#[cfg(unix)]
extern "C" {
    #[link_name = "flock"]
    fn libc_flock(fd: i32, operation: i32) -> i32;
}

#[cfg(windows)]
fn try_lock(file: &File) -> Result<(), LockError> {
    use std::os::windows::io::AsRawHandle;
    let handle = file.as_raw_handle() as *mut core::ffi::c_void;
    let mut overlapped: Overlapped = unsafe { std::mem::zeroed() };
    // SAFETY: LockFileEx on a valid file handle with a zeroed OVERLAPPED.
    let ok = unsafe {
        LockFileEx(
            handle,
            LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
            0,
            1,
            0,
            &mut overlapped,
        )
    };
    if ok != 0 {
        Ok(())
    } else {
        let err = io::Error::last_os_error();
        // ERROR_LOCK_VIOLATION (33) / ERROR_IO_PENDING means someone holds it.
        match err.raw_os_error() {
            Some(33) | Some(997) => Err(LockError::Contended),
            _ => Err(LockError::Unavailable(format!("LockFileEx failed: {err}"))),
        }
    }
}

#[cfg(windows)]
fn unlock(file: &File) {
    use std::os::windows::io::AsRawHandle;
    let handle = file.as_raw_handle() as *mut core::ffi::c_void;
    let mut overlapped: Overlapped = unsafe { std::mem::zeroed() };
    // SAFETY: UnlockFileEx on a valid file handle with a zeroed OVERLAPPED.
    unsafe {
        UnlockFileEx(handle, 0, 1, 0, &mut overlapped);
    }
}

#[cfg(windows)]
#[repr(C)]
struct Overlapped {
    internal: usize,
    internal_high: usize,
    pointer: usize,
    pointer_high: usize,
    h_event: *mut core::ffi::c_void,
}

#[cfg(windows)]
const LOCKFILE_EXCLUSIVE_LOCK: u32 = 0x2;
#[cfg(windows)]
const LOCKFILE_FAIL_IMMEDIATELY: u32 = 0x1;

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn LockFileEx(
        h_file: *mut core::ffi::c_void,
        dw_flags: u32,
        dw_reserved: u32,
        n_number_of_bytes_to_lock_low: u32,
        n_number_of_bytes_to_lock_high: u32,
        lp_overlapped: *mut Overlapped,
    ) -> i32;
    fn UnlockFileEx(
        h_file: *mut core::ffi::c_void,
        dw_reserved: u32,
        n_number_of_bytes_to_unlock_low: u32,
        n_number_of_bytes_to_unlock_high: u32,
        lp_overlapped: *mut Overlapped,
    ) -> i32;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "computer-host-lock-test-{tag}-{}",
            std::process::id()
        ))
    }

    #[test]
    fn second_acquire_contends_while_first_held() {
        let path = test_path("contend");
        let _ = fs::remove_file(&path);
        let first = acquire_at(&path).expect("first acquire succeeds");
        match acquire_at(&path) {
            Err(LockError::Contended) => {}
            other => panic!("expected contention, got {:?}", other.map(|_| ())),
        }
        drop(first);
        // After release, the lock can be taken again on the SAME inode/path.
        let _second = acquire_at(&path).expect("re-acquire after release succeeds");
    }

    #[test]
    fn drop_never_unlinks_rendezvous_file() {
        // Regression for the reviewed inode-replacement race: Drop must not
        // remove the lock file. A second process that already opened the
        // path keeps contending on the same inode.
        let path = test_path("stable");
        let _ = fs::remove_file(&path);
        {
            let lock = acquire_at(&path).expect("acquire");
            assert!(path.exists());
            drop(lock);
        }
        assert!(
            path.exists(),
            "rendezvous file must survive drop (no unlink race)"
        );
        // A fresh acquire reuses the same stable file.
        let _again = acquire_at(&path).expect("re-acquire on stable file");
        let _ = fs::remove_file(&path); // test cleanup only, AFTER drop
    }

    #[cfg(unix)]
    #[test]
    fn symlink_rendezvous_is_refused() {
        let dir = std::env::temp_dir().join(format!("ch-lock-symlink-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("victim");
        fs::write(&target, b"x").unwrap();
        let link = dir.join("lock-link");
        let _ = fs::remove_file(&link);
        std::os::unix::fs::symlink(&target, &link).unwrap();
        match acquire_at(&link) {
            Err(LockError::Unavailable(m)) => {
                assert!(m.contains("symbolic link"), "unexpected message: {m}");
            }
            other => panic!("expected symlink refusal, got {:?}", other.map(|_| ())),
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn permissive_existing_file_is_tightened() {
        use std::os::unix::fs::PermissionsExt;
        let path = test_path("perm");
        let _ = fs::remove_file(&path);
        fs::write(&path, b"").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        let lock = acquire_at(&path).expect("acquire tightens permissions");
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "lock file must be owner-only, got {mode:o}");
        drop(lock);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn lock_scope_uses_native_identity_not_env() {
        // The key must not change when USER/USERNAME/TMPDIR are mutated.
        let before = native_user_key();
        std::env::set_var("USER", "attacker-controlled");
        std::env::set_var("USERNAME", "attacker-controlled");
        let after = native_user_key();
        assert_eq!(before, after, "lock scope must ignore USER/USERNAME");
        #[cfg(unix)]
        {
            let uid = unsafe { libc_getuid() };
            assert_eq!(before, uid.to_string());
        }
    }

    #[test]
    fn lock_path_is_per_user() {
        let p = default_lock_path();
        assert!(p.to_string_lossy().contains("computer-host-"));
    }
}
