//! Sole-owner readers of std::process output pipes, not arbitrary Windows handles.
//! rustc 1.98.1 / 48a229cea: sys/process/windows.rs Stdio::MakePipe uses
//! child_pipe.rs::child_pipe (async parent handle, byte stream). ChildPipe::read
//! uses ReadFileEx + alertable completion, so CancelSynchronousIo cannot cancel it.
//! Peek only on these async handles; read no more than the available bytes.
//! Source/docs snapshots: .agents/runs/windows-stdout-disconnect-repair-sol-20261001.
//! Recheck this implementation assumption when upgrading the frozen toolchain.
use std::io::{self, Read};
use std::os::windows::io::{AsRawHandle, RawHandle};
use std::process::{ChildStderr, ChildStdout};
use std::ptr;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::Sender,
    Arc,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    Out,
    Err,
}
#[derive(Debug)]
pub enum Message {
    Data(Stream, Vec<u8>),
    End(Stream),
    // The sole owner dropped its read handle intentionally; NOT natural pipe EOF.
    ParentClosed(Stream),
    Error(Stream, String),
}
#[link(name = "kernel32")]
extern "system" {
    fn PeekNamedPipe(
        pipe: RawHandle,
        buffer: *mut std::ffi::c_void,
        buffer_size: u32,
        bytes_read: *mut u32,
        total_available: *mut u32,
        bytes_left: *mut u32,
    ) -> i32;
}
// The only callers pass ChildStdout/ChildStderr taken from Stdio::piped(). Do
// not generalize to File/io::pipe: PeekNamedPipe can block on synchronous handles.
pub(super) trait ChildOutput: Read + AsRawHandle + Send + 'static {}
impl ChildOutput for ChildStdout {}
impl ChildOutput for ChildStderr {}

// None is actual broken-pipe EOF. All other failures, including
// ERROR_OPERATION_ABORTED (995), remain errors even if a stop was requested.
fn available(pipe: &impl ChildOutput) -> io::Result<Option<usize>> {
    let mut bytes = 0;
    // SAFETY: sole owner keeps the pipe alive; this only queries the number of
    // queued bytes, writes one u32, and submits no outstanding read buffer.
    let ok = unsafe {
        PeekNamedPipe(
            pipe.as_raw_handle(),
            ptr::null_mut(),
            0,
            ptr::null_mut(),
            &mut bytes,
            ptr::null_mut(),
        )
    };
    if ok == 0 {
        // Capture GetLastError before any other call can overwrite it.
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(109) {
            // ERROR_BROKEN_PIPE
            Ok(None)
        } else {
            Err(error)
        }
    } else {
        Ok(Some(bytes as usize))
    }
}

pub struct Thread {
    handle: Option<JoinHandle<()>>,
    stop: Arc<AtomicBool>,
}
impl Thread {
    pub fn spawn(f: impl FnOnce(Arc<AtomicBool>) + Send + 'static) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let shared = stop.clone();
        Self {
            handle: Some(thread::spawn(move || f(shared))),
            stop,
        }
    }
    pub(super) fn reader(mut pipe: impl ChildOutput, stream: Stream, tx: Sender<Message>) -> Self {
        Self::spawn(move |stop| {
            let mut total = 0;
            let mut buf = [0u8; 8192];
            let terminal = loop {
                if stop.load(Ordering::SeqCst) {
                    break Message::ParentClosed(stream);
                }
                let bytes = match available(&pipe) {
                    Ok(None) => break Message::End(stream),
                    Ok(Some(bytes)) => bytes,
                    Err(error) => break Message::Error(stream, format!("PeekNamedPipe: {error}")),
                };
                if bytes == 0 {
                    // No read is pending. Stop unparks this thread immediately;
                    // the timeout only polls for new data without busy-spinning.
                    thread::park_timeout(Duration::from_millis(2));
                    continue;
                }
                if stop.load(Ordering::SeqCst) {
                    break Message::ParentClosed(stream);
                }
                // Sole read owner: no competing consumer can drain bytes between
                // peek and read. Never request future bytes from an idle writer.
                let count = bytes.min(buf.len());
                match pipe.read(&mut buf[..count]) {
                    Ok(0) => break Message::End(stream),
                    Ok(n) => {
                        total += n;
                        if total > 1024 * 1024 {
                            break Message::Error(stream, "stream exceeded 1 MiB".into());
                        }
                        if tx.send(Message::Data(stream, buf[..n].to_vec())).is_err() {
                            return;
                        }
                    }
                    Err(e) => break Message::Error(stream, e.to_string()),
                }
            };
            // This reader owns the ONLY parent read handle (taken from Child, never
            // cloned). ACK is sent only AFTER the real handle is closed. The caller
            // additionally joins this thread; dropping some other handle is insufficient.
            drop(pipe);
            let _ = tx.send(terminal);
        })
    }
    pub fn join_bounded(&mut self, bound: Duration, cancel: bool) -> bool {
        if cancel {
            // Cooperative stop only. Generic writer closures still have a bounded
            // join; Process RAII terminates its exact child before joining them.
            self.stop.store(true, Ordering::SeqCst);
            if let Some(handle) = &self.handle {
                handle.thread().unpark();
            }
        }
        let deadline = Instant::now() + bound;
        while let Some(handle) = &self.handle {
            if handle.is_finished() {
                return self.handle.take().unwrap().join().is_ok();
            }
            if Instant::now() >= deadline {
                return false;
            }
            thread::sleep(Duration::from_millis(2));
        }
        true
    }
}
impl Drop for Thread {
    fn drop(&mut self) {
        if !self.join_bounded(Duration::from_secs(2), true) {
            eprintln!("stdio EOF owned I/O thread did not join within cleanup bound");
        }
    }
}
