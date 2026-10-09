// Shared only by #[cfg(test)] TLS tests and the remote integration test binary.
// These are public throwaway fixtures, NOT deployment credentials. No CA keys.
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const EMBEDDED: &[(&str, &[u8])] = &[
    (
        "good/ca.pem",
        include_bytes!("../fixtures/remote/good/ca.pem"),
    ),
    (
        "good/server.pem",
        include_bytes!("../fixtures/remote/good/server.pem"),
    ),
    (
        "good/server.key",
        include_bytes!("../fixtures/remote/good/server.key"),
    ),
    (
        "good/client.token",
        include_bytes!("../fixtures/remote/good/client.token"),
    ),
    (
        "good/host.token",
        include_bytes!("../fixtures/remote/good/host.token"),
    ),
    (
        "wrong/ca.pem",
        include_bytes!("../fixtures/remote/wrong/ca.pem"),
    ),
    (
        "wrong/server.pem",
        include_bytes!("../fixtures/remote/wrong/server.pem"),
    ),
    (
        "wrong/server.key",
        include_bytes!("../fixtures/remote/wrong/server.key"),
    ),
];
static NEXT: AtomicU64 = AtomicU64::new(0);

pub struct Fixtures {
    root: PathBuf,
}

impl Fixtures {
    pub fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("test harness failure: clock before epoch")
            .as_nanos();
        for _ in 0..128 {
            let root = std::env::temp_dir().join(format!(
                "rpa-remote-test-{}-{stamp}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&root) {
                Ok(()) => {
                    // Acquire ownership before any fallible writes, including panic unwinding.
                    let assets = Self { root };
                    for dir in ["good", "wrong"] {
                        fs::create_dir(assets.root.join(dir)).unwrap_or_else(|e| {
                            panic!("test harness failure: create fixture subdirectory: {e}")
                        });
                    }
                    for &(name, bytes) in EMBEDDED {
                        write_new(&assets.root.join(name), bytes);
                    }
                    return assets;
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("test harness failure: create owned fixture directory: {e}"),
            }
        }
        panic!("test harness failure: exhausted unique fixture directory attempts");
    }

    /// Unknown, missing or modified fixtures fail the harness BEFORE TLS is tested.
    pub fn path(&self, name: &str) -> PathBuf {
        let expected = EMBEDDED
            .iter()
            .find(|(n, _)| *n == name)
            .unwrap_or_else(|| panic!("test harness failure: unknown fixture {name}"))
            .1;
        let path = self.root.join(name);
        let bytes = fs::read(&path)
            .unwrap_or_else(|e| panic!("test harness failure: unreadable fixture {name}: {e}"));
        assert!(
            bytes == expected,
            "test harness failure: modified fixture {name}"
        );
        path
    }

    /// Additional negative-test data; never overwrite, traverse or address a stream.
    pub fn create_file(&self, name: &str, bytes: &[u8]) -> PathBuf {
        assert!(
            !name.is_empty()
                && name != "."
                && name != ".."
                && name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)),
            "test harness failure: invalid owned file name"
        );
        let path = self.root.join(name);
        write_new(&path, bytes);
        path
    }
}

fn write_new(path: &Path, bytes: &[u8]) {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .unwrap_or_else(|e| panic!("test harness failure: create {}: {e}", path.display()));
    file.write_all(bytes)
        .unwrap_or_else(|e| panic!("test harness failure: write {}: {e}", path.display()));
}

impl Drop for Fixtures {
    fn drop(&mut self) {
        if let Err(e) = fs::remove_dir_all(&self.root) {
            // Do not cause a double panic during a failing test's unwind.
            eprintln!(
                "test harness cleanup failed for {}: {e}",
                self.root.display()
            );
        }
    }
}

#[cfg(test)]
#[path = "remote_assets_tests.rs"]
mod tests;
