use rpa_desktop_feedback::{
    codec::encode_renderer,
    protocol::{Heartbeat, RendererMessage, Session, Stop, V1},
    Diagnostic,
};
use rpa_desktop_feedback_linux::ipc::{Outbox, WriteProgress, OUTPUT_CAPACITY};
use std::io::{self, Write};

fn heartbeat() -> RendererMessage {
    RendererMessage::Heartbeat(Heartbeat { version: V1 })
}
fn stop() -> RendererMessage {
    RendererMessage::Stop(Stop {
        version: V1,
        session: Session {
            id: "session-a".into(),
            generation: u64::MAX,
        },
    })
}
struct ShortWriter {
    bytes: Vec<u8>,
    limit: usize,
    calls: usize,
}
impl Write for ShortWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.calls += 1;
        let n = bytes.len().min(self.limit);
        self.bytes.extend_from_slice(&bytes[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
struct ErrorWriter(io::ErrorKind);
impl Write for ErrorWriter {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(self.0.into())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn idle_outbox_does_not_touch_writer() {
    let mut outbox = Outbox::new();
    let mut writer = ShortWriter {
        bytes: Vec::new(),
        limit: 2,
        calls: 0,
    };
    assert_eq!(outbox.pump_once(&mut writer).unwrap(), WriteProgress::Idle);
    assert_eq!(writer.calls, 0);
}

#[test]
fn partial_writes_preserve_fifo_frames_without_interleaving_or_busy_loop() {
    let mut outbox = Outbox::new();
    outbox.enqueue(heartbeat()).unwrap();
    outbox.enqueue(stop()).unwrap();
    let mut expected = encode_renderer(&heartbeat()).unwrap();
    expected.extend(encode_renderer(&stop()).unwrap());
    let mut writer = ShortWriter {
        bytes: Vec::new(),
        limit: 3,
        calls: 0,
    };
    for _ in 0..expected.len() + 1 {
        let before = writer.calls;
        let progress = outbox.pump_once(&mut writer).unwrap();
        assert!(writer.calls - before <= 1);
        if progress == WriteProgress::Idle {
            break;
        }
    }
    assert_eq!(writer.bytes, expected);
    assert_eq!(outbox.buffered_frames(), 0);
    assert_eq!(outbox.buffered_bytes(), 0);
}

#[test]
fn would_block_and_interrupted_retain_unsent_frame() {
    for kind in [io::ErrorKind::WouldBlock, io::ErrorKind::Interrupted] {
        let mut outbox = Outbox::new();
        outbox.enqueue(stop()).unwrap();
        let before = outbox.buffered_bytes();
        assert_eq!(
            outbox.pump_once(&mut ErrorWriter(kind)).unwrap(),
            WriteProgress::Pending
        );
        assert_eq!(outbox.buffered_bytes(), before);
        assert_eq!(outbox.buffered_frames(), 1);
    }
}

#[test]
fn capacity_is_32_including_the_partial_frame() {
    assert_eq!(OUTPUT_CAPACITY, 32);
    let mut outbox = Outbox::new();
    outbox.enqueue(heartbeat()).unwrap();
    let mut writer = ShortWriter {
        bytes: Vec::new(),
        limit: 1,
        calls: 0,
    };
    assert_eq!(
        outbox.pump_once(&mut writer).unwrap(),
        WriteProgress::Pending
    );
    for _ in 1..OUTPUT_CAPACITY {
        outbox.enqueue(heartbeat()).unwrap();
    }
    assert_eq!(outbox.buffered_frames(), OUTPUT_CAPACITY);
    assert!(outbox.buffered_bytes() <= OUTPUT_CAPACITY * 16384);
    // Never silently drop a Stop on overload. Fail the renderer connection instead.
    assert_eq!(outbox.enqueue(stop()), Err(Diagnostic::ControlOverflow));
    assert_eq!(
        outbox.enqueue(heartbeat()),
        Err(Diagnostic::ControlOverflow)
    );
}

#[test]
fn zero_write_and_broken_pipe_poison_output_without_retries() {
    for zero_write in [false, true] {
        let mut outbox = Outbox::new();
        outbox.enqueue(stop()).unwrap();
        let result = if zero_write {
            outbox.pump_once(&mut ShortWriter {
                bytes: Vec::new(),
                limit: 0,
                calls: 0,
            })
        } else {
            outbox.pump_once(&mut ErrorWriter(io::ErrorKind::BrokenPipe))
        };
        assert_eq!(result, Err(Diagnostic::TransportLost));
        let mut writer = ShortWriter {
            bytes: Vec::new(),
            limit: 4096,
            calls: 0,
        };
        assert_eq!(
            outbox.pump_once(&mut writer),
            Err(Diagnostic::TransportLost)
        );
        assert_eq!(writer.calls, 0);
        assert_eq!(outbox.enqueue(heartbeat()), Err(Diagnostic::TransportLost));
    }
}
