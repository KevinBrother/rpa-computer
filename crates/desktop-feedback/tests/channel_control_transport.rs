use rpa_desktop_feedback::{channel::*, control::*, protocol::*, transport::*, Diagnostic};
use std::io::{self, Write};

fn snapshot(sequence: u64) -> Snapshot {
    Snapshot {
        version: V1,
        sequence,
        session: Some(session("a", 1)),
        phase: Phase::Idle,
        cleanup: Cleanup::NotNeeded,
        surface: None,
        pointer: None,
    }
}
fn session(id: &str, generation: u64) -> Session {
    Session {
        id: id.into(),
        generation,
    }
}

#[test]
fn fast_writer_slow_reader_retains_one_latest_snapshot_and_resynchronizes() {
    let publisher = Publisher::new(2).unwrap();
    let mut slow = publisher.subscribe().unwrap();
    for n in 1..=100_000 {
        publisher.try_publish(snapshot(n)).unwrap();
    }
    assert_eq!(publisher.retained_snapshot_count().unwrap(), 1);
    assert_eq!(slow.poll_latest().unwrap().unwrap().sequence, 100_000);
    assert!(slow.poll_latest().unwrap().is_none());
    let mut fresh = publisher.subscribe().unwrap();
    assert_eq!(fresh.poll_latest().unwrap().unwrap().sequence, 100_000);
    assert!(matches!(
        publisher.subscribe(),
        Err(Diagnostic::TooManySubscribers)
    ));
    drop(fresh);
    assert!(publisher.subscribe().is_ok());
}

#[test]
fn sequences_are_strict_and_rejected_publication_keeps_caller_snapshot() {
    let publisher = Publisher::new(1).unwrap();
    publisher.try_publish(snapshot(10)).unwrap();
    for n in [9, 10] {
        let error = publisher.try_publish(snapshot(n)).unwrap_err();
        assert_eq!(error.diagnostic, Diagnostic::SequenceRegression);
        assert_eq!(error.snapshot.sequence, n);
    }
    let mut cursor = SnapshotCursor::default();
    assert!(cursor.accept(&snapshot(10)).unwrap());
    assert!(!cursor.accept(&snapshot(10)).unwrap());
    assert!(!cursor.accept(&snapshot(9)).unwrap());
    assert!(cursor.accept(&snapshot(11)).unwrap());
    publisher.try_publish(snapshot(u64::MAX)).unwrap();
    assert_eq!(
        publisher.try_publish(snapshot(0)).unwrap_err().diagnostic,
        Diagnostic::SequenceExhausted
    );
}

#[test]
fn stop_is_not_authorized_by_sequence_and_revokes_before_cleanup_and_regrant() {
    let mut gate = ControlGate::default();
    gate.grant(session("a", 7)).unwrap();
    assert_eq!(
        gate.authorize_stop(&session("a", 6)),
        Err(StopRejection::WrongSession)
    );
    assert_eq!(
        gate.authorize_stop(&session("b", 7)),
        Err(StopRejection::WrongSession)
    );
    let token = gate.authorize_stop(&session("a", 7)).unwrap();
    assert!(gate.current().is_none());
    assert_eq!(
        gate.authorize_stop(&session("a", 7)),
        Err(StopRejection::NoControl)
    );
    assert_eq!(
        gate.grant(session("b", 1)),
        Err(StopRejection::CleanupPending)
    );
    assert_eq!(
        gate.finish_stop(&token, Cleanup::Failed),
        Err(StopRejection::CleanupUnconfirmed)
    );
    assert_eq!(
        gate.finish_stop(&token, Cleanup::Unknown),
        Err(StopRejection::CleanupUnconfirmed)
    );
    gate.finish_stop(&token, Cleanup::Released).unwrap();
    assert_eq!(
        gate.grant(session("a", 7)),
        Err(StopRejection::GenerationReused)
    );
    gate.grant(session("b", 1)).unwrap();
    assert_eq!(
        gate.finish_stop(&token, Cleanup::Released),
        Err(StopRejection::WrongStopToken)
    );
    assert_eq!(
        gate.authorize_stop(&session("a", 7)),
        Err(StopRejection::WrongSession)
    );
}

#[test]
fn queued_old_stop_is_checked_at_consumption_not_against_rendered_snapshot() {
    let inbox = StopInbox::new(2).unwrap();
    let mut gate = ControlGate::default();
    gate.grant(session("old", 1)).unwrap();
    inbox.try_push(session("old", 1)).unwrap();
    let old_token = gate.revoke_current().unwrap().unwrap();
    gate.finish_stop(&old_token, Cleanup::Released).unwrap();
    gate.grant(session("new", 1)).unwrap();
    assert_eq!(
        gate.authorize_stop(&inbox.try_pop().unwrap().unwrap()),
        Err(StopRejection::WrongSession)
    );
    inbox.try_push(session("new", 1)).unwrap();
    inbox.try_push(session("new", 1)).unwrap();
    assert_eq!(inbox.len().unwrap(), 1);
    inbox.try_push(session("old", 1)).unwrap();
    assert_eq!(
        inbox.try_push(session("third", 1)),
        Err(Diagnostic::ControlOverflow)
    );
    assert_eq!(inbox.len().unwrap(), 2);
}

#[test]
fn renderer_loss_can_revoke_current_without_untrusted_renderer_credentials() {
    let mut gate = ControlGate::default();
    assert!(gate.revoke_current().unwrap().is_none());
    gate.grant(session("a", 0)).unwrap();
    let token = gate.revoke_current().unwrap().unwrap();
    assert_eq!(token.session(), &session("a", 0));
    assert!(gate.current().is_none());
    assert_eq!(gate.revoke_current().unwrap(), Some(token));
}

#[test]
fn live_control_cannot_be_silently_replaced_by_another_grant() {
    let mut gate = ControlGate::default();
    gate.grant(session("a", 1)).unwrap();
    assert_eq!(
        gate.grant(session("b", 1)),
        Err(StopRejection::ControlAlreadyGranted)
    );
    assert_eq!(gate.current(), Some(&session("a", 1)));
}

struct SlowWriter {
    bytes: Vec<u8>,
    blocked: bool,
    chunk: usize,
}
impl Write for SlowWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.blocked {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        let n = bytes.len().min(self.chunk);
        self.bytes.extend_from_slice(&bytes[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn partial_frame_is_finished_before_new_snapshot_and_memory_stays_bounded() {
    let publisher = Publisher::new(1).unwrap();
    let mut pump = SnapshotWriter::new(publisher.subscribe().unwrap());
    let mut writer = SlowWriter {
        bytes: Vec::new(),
        blocked: false,
        chunk: 7,
    };
    publisher.try_publish(snapshot(1)).unwrap();
    assert_eq!(pump.pump(&mut writer).unwrap(), WriteProgress::Pending);
    writer.blocked = true;
    for n in 2..=10_000 {
        publisher.try_publish(snapshot(n)).unwrap();
        assert_eq!(pump.pump(&mut writer).unwrap(), WriteProgress::Pending);
        assert!(pump.buffered_bytes() <= rpa_desktop_feedback::codec::MAX_FRAME_BYTES);
    }
    writer.blocked = false;
    while pump.pump(&mut writer).unwrap() != WriteProgress::Sent(1) {}
    while pump.pump(&mut writer).unwrap() != WriteProgress::Sent(10_000) {}
    assert_eq!(pump.pump(&mut writer).unwrap(), WriteProgress::Idle);
    let lines: Vec<_> = writer
        .bytes
        .split(|b| *b == b'\n')
        .filter(|line| !line.is_empty())
        .map(
            |line| match rpa_desktop_feedback::codec::decode_host_line(line).unwrap() {
                HostMessage::Snapshot(s) => s.sequence,
            },
        )
        .collect();
    assert_eq!(lines, [1, 10_000]);
}

#[test]
fn write_failure_is_terminal_not_a_false_display_or_cleanup_confirmation() {
    let publisher = Publisher::new(1).unwrap();
    let mut pump = SnapshotWriter::new(publisher.subscribe().unwrap());
    publisher.try_publish(snapshot(1)).unwrap();
    assert_eq!(pump.pump(&mut io::sink()).unwrap(), WriteProgress::Sent(1));
    publisher.try_publish(snapshot(2)).unwrap();
    let mut zero = SlowWriter {
        bytes: Vec::new(),
        blocked: false,
        chunk: 0,
    };
    assert_eq!(pump.pump(&mut zero), Err(Diagnostic::TransportLost));
    assert_eq!(pump.pump(&mut io::sink()), Err(Diagnostic::TransportLost));
    let mut s = snapshot(3);
    s.phase = Phase::Closed;
    for cleanup in [Cleanup::Pending, Cleanup::Failed, Cleanup::Unknown] {
        s.cleanup = cleanup;
        assert!(!s.safe_completion());
    }
}
