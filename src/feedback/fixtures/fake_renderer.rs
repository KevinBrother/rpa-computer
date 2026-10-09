//! CC-only private protocol fixture. NEVER a real GUI/capture ready proof.
//! Build with rustc; mode is selected by copied executable filename so the
//! production adapter needs no test-only argv or sensitive environment.
use std::{
    io::{self, BufRead, Write},
    path::Path,
    thread,
    time::Duration,
};
fn emit(frame: &str) {
    let mut out = io::stdout().lock();
    writeln!(out, "{frame}").unwrap();
    out.flush().unwrap();
}
fn main() {
    let path = std::env::current_exe().unwrap();
    let mode = Path::new(&path)
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    if mode.contains("no_ready") {
        loop {
            thread::sleep(Duration::from_secs(1));
        }
    }
    if mode.contains("oversize") {
        emit(&"x".repeat(16_385));
        loop {
            thread::sleep(Duration::from_secs(1));
        }
    }
    if mode.contains("bad") {
        emit(r#"{"type":"ready","version":2}"#);
        return;
    }
    if mode.contains("unsupported") {
        emit(
            r#"{"type":"ready","version":1,"capture_exclusion":"unsupported","pointer_feedback":true}"#,
        );
    } else {
        emit(
            r#"{"type":"ready","version":1,"capture_exclusion":"requested","pointer_feedback":true}"#,
        );
    }
    if mode.contains("eof") {
        return;
    }
    if !mode.contains("heartbeat_loss") {
        thread::spawn(|| loop {
            thread::sleep(Duration::from_secs(1));
            emit(r#"{"type":"heartbeat","version":1}"#);
        });
    }
    if mode.contains("stall") {
        loop {
            thread::sleep(Duration::from_secs(1));
        }
    }
    let mut stopped = false;
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else {
            return;
        };
        if mode.contains("stop") && !stopped {
            // The Host emits bounded generated ASCII IDs and unsigned counters.
            if let Some(tail) = line.split("\"session\":{\"id\":\"").nth(1) {
                if let Some((id, tail)) = tail.split_once('"') {
                    if let Some(tail) = tail.strip_prefix(",\"generation\":") {
                        if let Some(gen) =
                            tail.split('}').next().and_then(|g| g.parse::<u64>().ok())
                        {
                            emit(&format!(
                                r#"{{"type":"stop","version":1,"session":{{"id":"{id}","generation":{gen}}}}}"#
                            ));
                            stopped = true;
                        }
                    }
                }
            }
        }
    }
}
