//! Headless transport fault fixtures for CC: actual Unix socket interruption,
//! no DISPLAY, GUI, native keyboard device or user clipboard. Added at GREEN;
//! these are supplemental to the immutable clipboard RED contract sources.
use super::{auth, control::Control, Worker};
use rpa_native_input::X11Lease;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn bounded_join<T>(join: std::thread::JoinHandle<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(1);
    while !join.is_finished() {
        assert!(
            Instant::now() < deadline,
            "transport thread failed to terminate after shutdown"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    join.join().ok().expect("transport thread panicked")
}

#[test]
fn watchdog_shutdown_interrupts_a_socket_without_a_waiting_driver_request() {
    let (socket, mut peer) = UnixStream::pair().unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
    let control = Arc::new(Control::default());
    control
        .arm(Instant::now() + Duration::from_millis(25))
        .unwrap();
    let watch = control.watch(socket.try_clone().unwrap()).unwrap();
    // This is an actual read; a recv_timeout-only implementation never produces EOF.
    assert_eq!(peer.read(&mut [0; 1]).unwrap(), 0);
    bounded_join(watch);
    assert!(control.stop.load(std::sync::atomic::Ordering::SeqCst));
    assert!(control.failure.get().is_some());
}

#[test]
fn watchdog_shutdown_interrupts_backpressured_writes_not_just_reply_waits() {
    let (mut socket, _unread_peer) = UnixStream::pair().unwrap();
    let control = Arc::new(Control::default());
    control
        .arm(Instant::now() + Duration::from_millis(25))
        .unwrap();
    let watch = control.watch(socket.try_clone().unwrap()).unwrap();
    let writer = std::thread::spawn(move || socket.write_all(&vec![0u8; 8 * 1024 * 1024]));
    assert!(bounded_join(writer).is_err());
    bounded_join(watch);
}

#[test]
fn blackholed_real_xcb_setup_is_interrupted_and_cannot_publish_after_timeout() {
    let (socket, mut peer) = UnixStream::pair().unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
    let start = Instant::now();
    let result = Worker::start_socket(
        socket,
        None,
        X11Lease::new(10, 20, [7; 16]),
        start + Duration::from_millis(30),
    );
    assert!(result.is_err());
    assert!(start.elapsed() < Duration::from_secs(1));
    let mut wire = Vec::new();
    peer.read_to_end(&mut wire).unwrap(); // EOF proves no surviving connected owner fd
    assert!(wire.len() >= 12, "real XCB must have sent a setup header");
    let endian = wire[0];
    let u16_at = |offset| {
        let bytes = [wire[offset], wire[offset + 1]];
        if endian == b'l' {
            u16::from_le_bytes(bytes) as usize
        } else {
            u16::from_be_bytes(bytes) as usize
        }
    };
    let padded = |n: usize| (n + 3) & !3;
    let handshake_len = 12 + padded(u16_at(6)) + padded(u16_at(8));
    assert_eq!(
        wire.len(),
        handshake_len,
        "no post-timeout X requests may reach the peer"
    );
}

#[test]
fn transport_only_accepts_explicit_local_display_addresses() {
    for (display, number) in [
        (":0", "0"),
        (":12.1", "12"),
        ("unix:2", "2"),
        ("unix/:3", "3"),
    ] {
        assert_eq!(auth::local_display_number(display).unwrap(), number);
    }
    for display in [
        "",
        ":",
        ":0.bad",
        "host:0",
        "localhost:10.0",
        ":65536",
        ":0/../x",
    ] {
        assert!(auth::local_display_number(display).is_err());
    }
}
