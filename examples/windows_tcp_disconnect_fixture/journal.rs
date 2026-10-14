//! Passive, bounded byte-for-byte taps around the REAL stdio loop.
//! Never changes frames, schedules work, waits for the parent, or requests stop.
use super::{common, identity};
use rpa_computer::mcp::jsonrpc::{MAX_LINE_BYTES, MAX_OUT_LINE_BYTES};
use serde_json::{json, Value};
use std::{
    io::{self, Read, Write},
    path::PathBuf,
    sync::{Arc, Mutex},
};

#[derive(Default)]
pub struct OpenJournal {
    request_id: Option<Value>,
    pub session_id: Option<String>,
}
pub type Shared = Arc<Mutex<OpenJournal>>;

pub struct Input<R> {
    inner: R,
    line: Vec<u8>,
    state: Shared,
}
impl<R> Input<R> {
    pub fn new(inner: R, state: Shared) -> Self {
        Self {
            inner,
            line: Vec::new(),
            state,
        }
    }
}
impl<R: Read> Read for Input<R> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let count = self.inner.read(bytes)?;
        // The reader only needs to identify the first real open request. Once
        // seen, subsequent bytes (including partial frames and EOF) are untouched.
        if self.state.lock().unwrap().request_id.is_some() {
            return Ok(count);
        }
        for &byte in &bytes[..count] {
            if byte == b'\n' {
                let frame: Value = serde_json::from_slice(&self.line).map_err(io::Error::other)?;
                self.line.clear();
                if frame["method"] == "tools/call" && frame["params"]["name"] == "computer_open" {
                    let id = frame
                        .get("id")
                        .filter(|id| id.is_string() || id.is_number())
                        .ok_or_else(|| io::Error::other("child open lacks a request id"))?;
                    self.state.lock().unwrap().request_id = Some(id.clone());
                    break;
                }
            } else {
                if self.line.len() >= MAX_LINE_BYTES {
                    return Err(io::Error::other("input journal frame cap"));
                }
                self.line.push(byte);
            }
        }
        Ok(count)
    }
}

pub struct Output<W> {
    inner: W,
    line: Vec<u8>,
    state: Shared,
    path: PathBuf,
}
impl<W> Output<W> {
    pub fn new(inner: W, state: Shared, path: PathBuf) -> Self {
        Self {
            inner,
            line: Vec::new(),
            state,
            path,
        }
    }
}
impl<W: Write> Write for Output<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let count = self.inner.write(bytes)?;
        if self.state.lock().unwrap().session_id.is_some() {
            return Ok(count);
        }
        for &byte in &bytes[..count] {
            if byte == b'\n' {
                let frame: Value = serde_json::from_slice(&self.line).map_err(io::Error::other)?;
                self.line.clear();
                let expected = self.state.lock().unwrap().request_id.clone();
                if expected
                    .as_ref()
                    .is_some_and(|id| frame.get("id") == Some(id))
                {
                    if frame["result"]["isError"] != false {
                        return Err(io::Error::other("fixture child open did not succeed"));
                    }
                    let data: Value = frame["result"]["content"]
                        .as_array()
                        .and_then(|blocks| blocks.iter().find(|b| b["type"] == "text"))
                        .and_then(|block| block["text"].as_str())
                        .ok_or_else(|| io::Error::other("child open missing metadata"))
                        .and_then(|text| serde_json::from_str(text).map_err(io::Error::other))?;
                    if data["state"] != "ready" {
                        return Err(io::Error::other("child open not ready"));
                    }
                    let session = data["session_id"]
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .ok_or_else(|| io::Error::other("child open missing session"))?
                        .to_owned();
                    // Publish the actual response, not a reconstructed ID. The
                    // identity is checked by the parent against its owned handle.
                    common::publish(
                        &self.path,
                        &json!({"kind":"passive_child_open_response",
                        "identity":identity::current_identity()?,"qpc":identity::qpc_ticks()?,
                        "request_id":expected,"session_id":session,"response":frame}),
                    )?;
                    self.state.lock().unwrap().session_id = Some(session);
                    break;
                }
            } else {
                if self.line.len() >= MAX_OUT_LINE_BYTES {
                    return Err(io::Error::other("output journal frame cap"));
                }
                self.line.push(byte);
            }
        }
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
