//! RED for executable transport wiring, reusing—not replacing—the frozen codec.
mod common;
#[path = "../src/native_ui/mod.rs"]
mod native_ui;
use native_ui::*;
use rpa_desktop_feedback::{protocol::*, Diagnostic};
use std::io::{self, Cursor, Read, Write};

fn heartbeat() -> RendererMessage {
    RendererMessage::Heartbeat(Heartbeat { version: V1 })
}

#[test]
fn worker_reads_at_most_one_fixed_4k_chunk_per_turn() {
    struct Probe {
        calls: usize,
    }
    impl Read for Probe {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            self.calls += 1;
            assert_eq!(buffer.len(), 4096);
            Err(io::ErrorKind::WouldBlock.into())
        }
    }
    let mut reader = Probe { calls: 0 };
    let mut bridge = PipeBridge::new();
    bridge.read_once(&mut reader).unwrap();
    assert_eq!(reader.calls, 1);
    assert!(matches!(bridge.take_input(), Input::Idle));
}

#[test]
fn fragmented_input_uses_shared_decoder_and_one_latest_slot() {
    let mut bridge = PipeBridge::new();
    let frame = common::frame(1);
    bridge.read_once(&mut Cursor::new(&frame[..8])).unwrap();
    assert!(matches!(bridge.take_input(), Input::Idle));
    let mut rest = frame[8..].to_vec();
    rest.extend(common::frame(7));
    rest.extend(common::frame(3));
    bridge.read_once(&mut Cursor::new(rest)).unwrap();
    let Input::Snapshot(s) = bridge.take_input() else {
        panic!("latest snapshot required")
    };
    assert_eq!(s.sequence, 7);
    assert!(matches!(bridge.take_input(), Input::Idle));
}

#[test]
fn eof_supersedes_pending_snapshot_and_stays_terminal() {
    let mut bridge = PipeBridge::new();
    bridge
        .read_once(&mut Cursor::new(common::frame(1)))
        .unwrap();
    bridge.read_once(&mut io::empty()).unwrap();
    assert!(matches!(bridge.take_input(), Input::Eof));
    assert!(matches!(bridge.take_input(), Input::Eof));
    assert!(bridge.enqueue(heartbeat()).is_err());
}

#[test]
fn truncated_parent_eof_is_failure_not_graceful_shutdown() {
    let mut bridge = PipeBridge::new();
    bridge
        .read_once(&mut Cursor::new(b"{\"type\":".as_slice()))
        .unwrap();
    assert_eq!(
        bridge.read_once(&mut io::empty()),
        Err(Diagnostic::TruncatedLine)
    );
    assert!(matches!(
        bridge.take_input(),
        Input::Failed(Diagnostic::TruncatedLine)
    ));
}

#[test]
fn oversized_or_malformed_input_poison_the_mailbox_without_payload_echo() {
    for (bytes, expected) in [
        (vec![b'x'; 16384], Diagnostic::LineTooLong),
        (b"not-json-secret\n".to_vec(), Diagnostic::InvalidJson),
    ] {
        let mut bridge = PipeBridge::new();
        let mut reader = Cursor::new(bytes);
        for _ in 0..6 {
            if bridge.read_once(&mut reader).is_err() {
                break;
            }
        }
        assert_eq!(bridge.failure(), Some(expected));
        assert!(matches!(bridge.take_input(), Input::Failed(_)));
        assert!(!format!("{:?}", bridge.failure()).contains("secret"));
    }
}

#[test]
fn partial_output_writes_are_fifo_bounded_and_single_attempt() {
    struct Slow {
        bytes: Vec<u8>,
        calls: usize,
    }
    impl Write for Slow {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.calls += 1;
            let n = bytes.len().min(3);
            self.bytes.extend_from_slice(&bytes[..n]);
            Ok(n)
        }
        fn flush(&mut self) -> io::Result<()> {
            panic!("worker must not flush per frame")
        }
    }
    let mut bridge = PipeBridge::new();
    bridge.enqueue(heartbeat()).unwrap();
    let stop = RendererMessage::Stop(Stop {
        version: V1,
        session: common::snapshot(1).session.unwrap(),
    });
    bridge.enqueue(stop.clone()).unwrap();
    let mut writer = Slow {
        bytes: vec![],
        calls: 0,
    };
    bridge.write_once(&mut writer).unwrap();
    assert_eq!(writer.calls, 1);
    assert_eq!(writer.bytes.len(), 3);
    let mut expected = rpa_desktop_feedback::codec::encode_renderer(&heartbeat()).unwrap();
    expected.extend(rpa_desktop_feedback::codec::encode_renderer(&stop).unwrap());
    for _ in 0..expected.len() {
        bridge.write_once(&mut writer).unwrap();
    }
    assert_eq!(writer.bytes, expected);
    assert!(bridge.failure().is_none());
}

#[test]
fn writer_wouldblock_is_not_failure_but_broken_pipe_is_terminal() {
    struct Fault(io::ErrorKind);
    impl Write for Fault {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(self.0.into())
        }
        fn flush(&mut self) -> io::Result<()> {
            panic!("no flush")
        }
    }
    let mut bridge = PipeBridge::new();
    bridge.enqueue(heartbeat()).unwrap();
    bridge
        .write_once(&mut Fault(io::ErrorKind::WouldBlock))
        .unwrap();
    assert!(bridge.failure().is_none());
    assert_eq!(
        bridge.write_once(&mut Fault(io::ErrorKind::BrokenPipe)),
        Err(Diagnostic::TransportLost)
    );
    assert_eq!(bridge.failure(), Some(Diagnostic::TransportLost));
    assert!(bridge.enqueue(heartbeat()).is_err());
}

#[test]
fn output_capacity_includes_partial_frame_and_overflow_is_terminal() {
    let mut bridge = PipeBridge::new();
    for _ in 0..32 {
        bridge.enqueue(heartbeat()).unwrap();
    }
    struct OneByte;
    impl Write for OneByte {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            Ok(bytes.len().min(1))
        }
        fn flush(&mut self) -> io::Result<()> {
            panic!("no flush")
        }
    }
    bridge.write_once(&mut OneByte).unwrap();
    assert_eq!(
        bridge.enqueue(heartbeat()),
        Err(Diagnostic::ControlOverflow)
    );
    assert_eq!(bridge.failure(), Some(Diagnostic::ControlOverflow));
    assert!(matches!(
        bridge.take_input(),
        Input::Failed(Diagnostic::ControlOverflow)
    ));
}

#[test]
fn finish_is_idempotent_and_rejects_new_controls() {
    let mut bridge = PipeBridge::new();
    bridge.finish();
    bridge.finish();
    assert!(bridge.enqueue(heartbeat()).is_err());
}
