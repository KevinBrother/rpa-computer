//! Assertion-RED tests for the preserved legacy acquisition path. Independent
//! CC only, isolated Unix sockets; no Xvfb, GUI, input or environment mutation.
//! The scheduling hook holds a real owner reply boundary and observes only
//! socket shutdown, not the abandonment flag. It does not implement the fix.
use super::*;
use std::io::Read;
use std::net::Shutdown;
use std::sync::mpsc::{channel, Receiver, TryRecvError};

mod extra;
mod peer;
use peer::Listener;

const BUDGET: Duration = Duration::from_millis(400);

fn caller(
    display: String,
    probe: SetupProbe,
) -> std::thread::JoinHandle<Result<ServerLease, LockError>> {
    std::thread::spawn(move || ServerLease::acquire_display(display, BUDGET, probe))
}
fn assert_deadline(result: Result<ServerLease, LockError>) {
    match result {
        Err(LockError::Unavailable(message)) => assert!(message.contains("deadline"), "{message}"),
        Err(LockError::Contended) => panic!("fixture has no previous owner"),
        Ok(_) => panic!("cancelled acquisition must not publish authority"),
    }
}
fn confirm_test_cleanup(retired: &Receiver<()>, already_retired: bool) {
    if !already_retired {
        retired
            .recv_timeout(Duration::from_secs(2))
            .expect("fixture must unblock legacy setup before asserting RED");
    }
}

#[test]
#[ignore = "CC: isolated real Unix XCB handshake, no desktop/input"]
fn stalled_pre_handshake_must_close_socket_and_retire_before_return() {
    let listener = Listener::bind();
    let (retired_tx, retired_rx) = channel();
    let started = Instant::now();
    let call = caller(
        listener.display.clone(),
        SetupProbe {
            retired: Some(retired_tx),
            before_owner_reply: None,
        },
    );
    let mut peer = listener.accept(); // verifies actual Connection::connect setup reached the socket
    let result = call.join().unwrap();
    let elapsed_at_return = started.elapsed();
    let retired_at_return = retired_rx.try_recv().is_ok();
    let mut trailing = Vec::new();
    let eof_at_return = peer.socket.read_to_end(&mut trailing).is_ok();
    // Supply the delayed setup only AFTER the caller has returned. A fixed
    // implementation has already shut down this connection and joined setup.
    let late_setup_rejected = peer.setup().is_err();
    let _ = peer.socket.shutdown(Shutdown::Both);
    confirm_test_cleanup(&retired_rx, retired_at_return);

    assert_deadline(result);
    assert!(
        elapsed_at_return < Duration::from_secs(2),
        "400ms acquisition budget must also bound task retirement: {elapsed_at_return:?}"
    );
    assert!(retired_at_return, "deadline returned with the real XCB setup task still blocked; recv_timeout is not retirement");
    assert!(
        eof_at_return,
        "timeout must close the actual socket, not only abandon its receiver"
    );
    assert!(
        late_setup_rejected,
        "late setup reply was accepted after caller cancellation"
    );
    assert!(
        trailing.is_empty(),
        "no core protocol work is permitted after failed setup"
    );
}

#[test]
#[ignore = "CC: isolated real Unix protocol with controlled scheduling, no desktop/input"]
fn delayed_owner_reply_must_not_acquire_selection_after_caller_cancelled() {
    let listener = Listener::bind();
    let (retired_tx, retired_rx) = channel();
    let (at_reply_tx, at_reply_rx) = channel();
    let (release_tx, release_rx) = channel();
    let started = Instant::now();
    let call = caller(
        listener.display.clone(),
        SetupProbe {
            retired: Some(retired_tx),
            before_owner_reply: Some(Box::new(move |connection| {
                at_reply_tx.send(()).unwrap();
                let until = Instant::now() + Duration::from_secs(3);
                loop {
                    match release_rx.try_recv() {
                        Ok(()) | Err(TryRecvError::Disconnected) => return,
                        Err(TryRecvError::Empty) => {}
                    }
                    // A future real shutdown+join implementation must be able to
                    // release this scheduling gate via the underlying socket.
                    let mut fd = libc::pollfd {
                        fd: connection.as_raw_fd(),
                        events: libc::POLLRDHUP,
                        revents: 0,
                    };
                    unsafe {
                        libc::poll(&mut fd, 1, 5);
                    }
                    if fd.revents
                        & (libc::POLLHUP | libc::POLLERR | libc::POLLRDHUP | libc::POLLNVAL)
                        != 0
                    {
                        return;
                    }
                    assert!(
                        Instant::now() < until,
                        "fixture gate did not receive release or socket shutdown"
                    );
                }
            })),
        },
    );
    let mut peer = listener.accept();
    peer.setup().unwrap();
    let mut saw_grab = false;
    let mut atom_id = 100;
    loop {
        let request = peer
            .request()
            .expect("fixture protocol before owner query")
            .expect("setup must reach owner query");
        match request[0] {
            16 => {
                // InternAtom
                atom_id += 1;
                peer.reply_value(atom_id).unwrap();
            }
            1 | 18 => {} // CreateWindow, ChangeProperty (no replies)
            43 => {
                peer.reply_value(0).unwrap();
            } // GetInputFocus barrier
            36 => {
                saw_grab = true;
            }
            23 => break, // Withhold the first GetSelectionOwner reply.
            opcode => panic!("unexpected fixture request opcode {opcode}"),
        }
    }
    at_reply_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    let result = call.join().unwrap(); // cancellation is now observable to caller
    let elapsed_at_return = started.elapsed();
    let retired_at_return = retired_rx.try_recv().is_ok();
    // Make the legitimate owner=None reply readable BEFORE rescheduling the
    // task. Legacy reply() consumes ready replies before checking its deadline.
    let delayed_reply_written = peer.reply_value(0).is_ok();
    let _ = release_tx.send(());
    let mut late_set_owner = false;
    let mut owner = 0;
    loop {
        match peer.request() {
            Ok(Some(request)) => match request[0] {
                22 => {
                    late_set_owner = true;
                    owner = peer.read32(&request, 4);
                }
                23 => {
                    let _ = peer.reply_value(owner);
                }
                43 => {
                    let _ = peer.reply_value(0);
                }
                37 => {} // UngrabServer during cleanup is not acquisition.
                opcode => panic!("unexpected post-cancellation request opcode {opcode}"),
            },
            Ok(None) => break,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::BrokenPipe
                ) =>
            {
                break
            }
            Err(e) => panic!("fixture did not reach EOF after release: {e}"),
        }
    }
    let _ = peer.socket.shutdown(Shutdown::Both);
    confirm_test_cleanup(&retired_rx, retired_at_return);

    assert!(
        saw_grab,
        "fixture must exercise acquisition inside a real GrabServer protocol sequence"
    );
    assert_deadline(result);
    assert!(
        elapsed_at_return < Duration::from_secs(2),
        "400ms acquisition budget must also bound task retirement: {elapsed_at_return:?}"
    );
    assert!(!late_set_owner, "SetSelectionOwner was sent AFTER caller cancellation (delayed_reply_written={delayed_reply_written}, retired_at_return={retired_at_return})");
    assert!(
        retired_at_return,
        "caller returned without retiring the protocol task"
    );
}
