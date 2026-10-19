//! Additional independent coverage for the same cancellation mechanism. These
//! assertions were not part of the two independently confirmed RED cases.
use super::*;

fn until_owner_query(peer: &mut peer::Peer) {
    peer.setup().unwrap();
    let mut atom = 100;
    let mut grabbed = false;
    loop {
        let request = peer
            .request()
            .unwrap()
            .expect("setup must reach owner query");
        match request[0] {
            16 => {
                atom += 1;
                peer.reply_value(atom).unwrap();
            }
            1 | 18 => {}
            43 => peer.reply_value(0).unwrap(),
            36 => grabbed = true,
            23 => {
                assert!(grabbed);
                return;
            }
            opcode => panic!("unexpected setup opcode {opcode}"),
        }
    }
}

#[test]
#[ignore = "CC: isolated real Unix protocol, no desktop/input"]
fn stalled_grab_reply_retires_at_short_grab_deadline() {
    let listener = Listener::bind();
    let display = listener.display.clone();
    let (retired_tx, retired_rx) = channel();
    let started = Instant::now();
    let call = std::thread::spawn(move || {
        ServerLease::acquire_display(
            display,
            Duration::from_secs(3),
            SetupProbe {
                retired: Some(retired_tx),
                before_owner_reply: None,
            },
        )
    });
    let mut peer = listener.accept();
    until_owner_query(&mut peer);
    let result = call.join().unwrap();
    let elapsed = started.elapsed();
    let retired = retired_rx.try_recv().is_ok();
    let mut trailing = Vec::new();
    let closed = peer.socket.read_to_end(&mut trailing).is_ok();
    let _ = peer.socket.shutdown(Shutdown::Both);
    confirm_test_cleanup(&retired_rx, retired);
    assert_deadline(result);
    assert!(
        elapsed < Duration::from_millis(1500),
        "500ms grab must not wait for 3s setup budget: {elapsed:?}"
    );
    assert!(
        retired && closed,
        "grab failure must retire setup and close socket"
    );
}

#[test]
#[ignore = "CC: isolated real Unix backpressure, no desktop/input"]
fn blocked_flush_is_interrupted_and_joined_within_grab_budget() {
    let listener = Listener::bind();
    let display = listener.display.clone();
    let (retired_tx, retired_rx) = channel();
    let (flush_error_tx, flush_error_rx) = channel();
    let started = Instant::now();
    let call = std::thread::spawn(move || {
        ServerLease::acquire_display(
            display,
            Duration::from_secs(3),
            SetupProbe {
                retired: Some(retired_tx),
                before_owner_reply: Some(Box::new(move |connection| {
                    let size: libc::c_int = 4096;
                    // SAFETY: live socket, valid integer option and matching size.
                    assert_eq!(
                        unsafe {
                            libc::setsockopt(
                                connection.as_raw_fd(),
                                libc::SOL_SOCKET,
                                libc::SO_SNDBUF,
                                (&size as *const libc::c_int).cast(),
                                std::mem::size_of_val(&size) as libc::socklen_t,
                            )
                        },
                        0
                    );
                    // The peer stops reading at the owner query. Fill the REAL
                    // socket with harmless core NoOperations, not simulated waits.
                    // Both send_request's implicit flush and explicit flush below
                    // must be interruptible by the retained duplicate's shutdown.
                    for _ in 0..4096 {
                        for _ in 0..256 {
                            connection.send_request(&xcb::x::NoOperation {});
                        }
                        if connection.flush().is_err() {
                            let _ = flush_error_tx.send(());
                            return;
                        }
                    }
                    panic!("fixture failed to establish socket backpressure");
                })),
            },
        )
    });
    let mut peer = listener.accept();
    until_owner_query(&mut peer);
    let result = call.join().unwrap();
    let elapsed = started.elapsed();
    let retired = retired_rx.try_recv().is_ok();
    let interrupted = flush_error_rx.try_recv().is_ok();
    let mut trailing = Vec::new();
    let closed = peer.socket.read_to_end(&mut trailing).is_ok();
    let _ = peer.socket.shutdown(Shutdown::Both);
    confirm_test_cleanup(&retired_rx, retired);
    assert_deadline(result);
    assert!(
        elapsed < Duration::from_millis(1500),
        "blocked flush escaped 500ms grab watchdog: {elapsed:?}"
    );
    assert!(interrupted, "real XCB flush must observe shutdown failure");
    assert!(
        retired && closed,
        "blocked flush task/socket must retire before caller return"
    );
}
