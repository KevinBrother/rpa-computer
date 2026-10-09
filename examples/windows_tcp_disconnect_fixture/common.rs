//! Test-only shared identity and bounded, atomic evidence files. No TLS secrets.
use serde_json::{json, Value};
use std::{fs, io, io::Write, path::Path, sync::mpsc, thread, time::Duration};
pub const BUILD_ID: &str = match option_env!("RPA_TCP_DISCONNECT_BUILD_ID") {
    Some(v) => v,
    None => "UNFROZEN_BUILD",
};
pub fn version() -> String {
    format!(
        "rpa-computer/{};windows-tcp-disconnect-v1;{BUILD_ID}",
        env!("CARGO_PKG_VERSION")
    )
}
pub fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    f.write_all(bytes)?;
    f.flush()
}
pub fn publish(path: &Path, value: &Value) -> io::Result<()> {
    if path.exists() {
        return Err(io::Error::other("evidence must not be overwritten"));
    }
    let tmp = path.with_extension("pending");
    write_new(&tmp, &serde_json::to_vec(value)?)?;
    fs::rename(tmp, path)
}
pub fn read(path: &Path) -> io::Result<Value> {
    if fs::metadata(path)?.len() > 128 * 1024 {
        return Err(io::Error::other("evidence >128KiB"));
    }
    serde_json::from_slice(&fs::read(path)?).map_err(io::Error::other)
}
pub fn context() -> io::Result<(std::path::PathBuf, String)> {
    if BUILD_ID == "UNFROZEN_BUILD" {
        return Err(io::Error::other(
            "RPA_TCP_DISCONNECT_BUILD_ID missing at compile time",
        ));
    }
    let dir = std::path::PathBuf::from(
        std::env::var_os("RPA_TCP_CASE_DIR").ok_or_else(|| io::Error::other("missing case dir"))?,
    );
    let nonce = std::env::var("RPA_TCP_NONCE").map_err(io::Error::other)?;
    if !dir.is_absolute() || !dir.is_dir() || nonce.is_empty() {
        return Err(io::Error::other("invalid owned context"));
    }
    Ok((dir, nonce))
}
/// Hard failure only: no cancel/shutdown flag. Stops only this exact process.
/// The marker and native71 make a watchdog exit impossible to mistake for GREEN.
pub struct Watchdog {
    done: Option<mpsc::Sender<()>>,
    thread: Option<thread::JoinHandle<()>>,
}
impl Watchdog {
    pub fn start(path: std::path::PathBuf) -> Self {
        let (tx, rx) = mpsc::channel();
        let h = thread::spawn(move || {
            if rx.recv_timeout(Duration::from_secs(75)) == Err(mpsc::RecvTimeoutError::Timeout) {
                let _ = publish(
                    &path,
                    &json!({"kind":"watchdog_failure","pid":std::process::id(),"limit_secs":75}),
                );
                std::process::exit(71);
            }
        });
        Self {
            done: Some(tx),
            thread: Some(h),
        }
    }
    pub fn finish(&mut self) {
        if let Some(tx) = self.done.take() {
            let _ = tx.send(());
        }
        if let Some(h) = self.thread.take() {
            h.join().expect("watchdog join");
        }
    }
}
impl Drop for Watchdog {
    fn drop(&mut self) {
        self.finish();
    }
}
