//! Inbox contracts exercise real shared decoding once the adapter is implemented.
mod common;
use common::frame;
use rpa_desktop_feedback::{
    codec::{MAX_FRAME_BYTES, MAX_LINE_BYTES},
    Diagnostic,
};
use rpa_desktop_feedback_linux::ipc::Inbox;

#[test]
fn arbitrary_chunk_boundaries_decode_one_snapshot() {
    let bytes = frame(7);
    for chunk_size in [1, 2, 7, 4096] {
        let mut inbox = Inbox::new();
        for chunk in bytes.chunks(chunk_size) {
            inbox.feed(chunk).unwrap();
        }
        assert_eq!(inbox.take_latest().unwrap().sequence, 7);
        assert!(inbox.take_latest().is_none());
    }
}

#[test]
fn burst_coalesces_to_one_slot_without_losing_high_watermark_after_take() {
    let mut inbox = Inbox::new();
    for sequence in 0..1000 {
        inbox.feed(&frame(sequence)).unwrap();
        assert_eq!(inbox.pending_snapshots(), 1);
    }
    assert_eq!(inbox.take_latest().unwrap().sequence, 999);
    for sequence in [998, 999, 0] {
        inbox.feed(&frame(sequence)).unwrap();
    }
    assert!(inbox.take_latest().is_none());
    inbox.feed(&frame(1000)).unwrap();
    assert_eq!(inbox.take_latest().unwrap().sequence, 1000);
}

#[test]
fn exact_16k_frame_and_crlf_account_for_lf_and_cr() {
    for crlf in [false, true] {
        let mut bytes = frame(1);
        bytes.pop();
        let content_limit = MAX_LINE_BYTES - usize::from(crlf);
        bytes.resize(content_limit, b' ');
        if crlf {
            bytes.push(b'\r');
        }
        bytes.push(b'\n');
        assert_eq!(bytes.len(), MAX_FRAME_BYTES);
        let mut inbox = Inbox::new();
        for chunk in bytes.chunks(4096) {
            inbox.feed(chunk).unwrap();
        }
        assert_eq!(inbox.take_latest().unwrap().sequence, 1);
    }
}

#[test]
fn oversized_unterminated_input_fails_without_waiting_for_newline() {
    let mut inbox = Inbox::new();
    inbox.feed(&vec![b' '; MAX_LINE_BYTES]).unwrap();
    assert_eq!(inbox.feed(b"x"), Err(Diagnostic::LineTooLong));
    assert!(inbox.buffered_bytes() <= MAX_LINE_BYTES);
    assert!(inbox.buffer_capacity() <= MAX_LINE_BYTES);
    assert_eq!(inbox.feed(&frame(2)), Err(Diagnostic::LineTooLong));
    assert!(inbox.take_latest().is_none());
}

#[test]
fn malformed_line_poisons_connection_and_discards_pending_snapshot() {
    let mut inbox = Inbox::new();
    let mut batch = frame(1);
    batch.extend_from_slice(b"{not-json}\n");
    batch.extend(frame(3));
    let error = inbox.feed(&batch).unwrap_err();
    assert!(inbox.take_latest().is_none());
    assert_eq!(inbox.feed(&frame(4)), Err(error));
    assert_eq!(inbox.finish(), Err(error));
}

#[test]
fn partial_eof_is_terminal_even_for_valid_json_without_lf() {
    let mut inbox = Inbox::new();
    let mut bytes = frame(1);
    bytes.pop();
    inbox.feed(&bytes).unwrap();
    assert_eq!(inbox.finish(), Err(Diagnostic::TruncatedLine));
    assert!(!inbox.is_eof());
    assert!(inbox.take_latest().is_none());
}

#[test]
fn clean_eof_discards_pending_ui_work_and_never_accepts_new_input() {
    let mut inbox = Inbox::new();
    inbox.feed(&frame(1)).unwrap();
    inbox.finish().unwrap();
    assert!(inbox.is_eof());
    assert!(inbox.take_latest().is_none());
    assert!(inbox.feed(&frame(2)).is_err());
}

#[test]
fn invalid_utf8_and_secrets_are_never_echoed_as_diagnostics() {
    for bytes in [
        b"\xff\n".as_slice(),
        b"private-password-not-json\n".as_slice(),
    ] {
        let mut inbox = Inbox::new();
        let error = inbox.feed(bytes).unwrap_err();
        assert_eq!(error, Diagnostic::InvalidJson);
        assert!(!error.to_string().contains("private-password"));
    }
}
