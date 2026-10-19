//! CC-only real-socket adversarial tests. These peers speak (or deliberately
//! stall) X11 setup over Unix sockets; they are not mocks of the timeout logic.
//! No GUI, input, environment mutation, or existing desktop is involved.
use super::*;
use std::io::{Read, Write};

fn assert_timeout(result: Result<Registration, String>) {
    match result {
        Ok(registration) => {
            registration.disarm();
            panic!("stalled X11 peer must never be reported armed");
        }
        Err(error) => assert!(
            error.contains("timed out") || error.contains("deadline"),
            "{error}"
        ),
    }
}

#[test]
#[ignore = "CC: real Unix socket, stalled X11 setup; no GUI/input"]
fn stalled_handshake_is_shutdown_joined_and_never_late_armed() {
    let (client, mut peer) = UnixStream::pair().unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let start = Instant::now();
    let result = arm_socket(
        client,
        None,
        Arc::clone(&cancel),
        start + Duration::from_millis(150),
    );
    assert_timeout(result);
    assert!(
        start.elapsed() < Duration::from_secs(1),
        "handshake thread was not interrupted/joined"
    );
    assert_eq!(Arc::strong_count(&cancel), 1);
    assert!(!cancel.load(Ordering::SeqCst));
    let mut received = Vec::new();
    peer.read_to_end(&mut received)
        .expect("watchdog must produce EOF, not leave a connected setup thread");
    assert_eq!(
        received.len(),
        12,
        "only the unauthenticated setup request, no later requests/grabs"
    );
    assert!(
        peer.write_all(&setup_reply(received[0])).is_err(),
        "late peer cannot revive an abandoned registration"
    );
}

// Minimal successful core-X11 setup, one root, no visuals. Sufficient for
// GetKeyboardMapping; no live desktop or input server is launched.
fn setup_reply(order: u8) -> Vec<u8> {
    assert!(order == b'l' || order == b'B');
    let mut bytes = vec![0u8; 80];
    bytes[0] = 1;
    let put16 = |bytes: &mut [u8], offset, value: u16| {
        bytes[offset..offset + 2].copy_from_slice(&if order == b'l' {
            value.to_le_bytes()
        } else {
            value.to_be_bytes()
        });
    };
    let put32 = |bytes: &mut [u8], offset, value: u32| {
        bytes[offset..offset + 4].copy_from_slice(&if order == b'l' {
            value.to_le_bytes()
        } else {
            value.to_be_bytes()
        });
    };
    put16(&mut bytes, 2, 11); // Protocol major.
    put16(&mut bytes, 6, 18); // 72 bytes follow the 8-byte header.
    put32(&mut bytes, 12, 0x200000); // Resource base.
    put32(&mut bytes, 16, 0x1fffff); // Resource mask.
    put16(&mut bytes, 26, 65535); // Maximum request length.
    bytes[28] = 1; // One screen, no pixmap formats/vendor string.
    bytes[34] = 8;
    bytes[35] = 255;
    put32(&mut bytes, 40, 0x100); // Root window.
    put16(&mut bytes, 60, 800);
    put16(&mut bytes, 62, 600);
    bytes[78] = 24; // Root depth.
    bytes
}

#[test]
#[ignore = "CC: real Unix socket, accepted setup then stalled map reply; no GUI/input"]
fn stalled_post_setup_reply_is_shutdown_before_any_grabs_and_allows_retry() {
    for _ in 0..3 {
        let (client, mut peer) = UnixStream::pair().unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let peer_thread = std::thread::spawn(move || {
            let mut setup = [0u8; 12];
            peer.read_exact(&mut setup).unwrap();
            peer.write_all(&setup_reply(setup[0])).unwrap();
            let mut requests = Vec::new();
            peer.read_to_end(&mut requests)
                .expect("deadline must shut down the actual connection");
            assert!(
                requests.len() >= 8,
                "client must get past the setup handshake"
            );
            assert_eq!(requests[0], 101, "GetKeyboardMapping must precede GrabKey");
            // First map wait never resolves, so no GrabKey request can be sent.
            assert_eq!(requests.len(), 8);
        });
        let cancel = Arc::new(AtomicBool::new(false));
        let start = Instant::now();
        assert_timeout(arm_socket(
            client,
            None,
            Arc::clone(&cancel),
            start + Duration::from_millis(200),
        ));
        assert!(start.elapsed() < Duration::from_secs(1));
        assert_eq!(Arc::strong_count(&cancel), 1);
        assert!(!cancel.load(Ordering::SeqCst));
        peer_thread.join().unwrap();
    }
}

#[test]
#[ignore = "CC: binary auth over real Unix socket; no desktop credentials/GUI/input"]
fn binary_auth_cookie_reaches_setup_without_c_string_truncation() {
    let (client, mut peer) = UnixStream::pair().unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    // Synthetic fixture bytes, deliberately containing several NULs.
    let cookie = [0, 1, 2, 0, 4, 5, 6, 7, 8, 9, 10, 0, 12, 13, 14, 255];
    let peer_thread = std::thread::spawn(move || {
        let mut header = [0u8; 12];
        peer.read_exact(&mut header).unwrap();
        let number = |offset| {
            let bytes = [header[offset], header[offset + 1]];
            usize::from(if header[0] == b'l' {
                u16::from_le_bytes(bytes)
            } else {
                u16::from_be_bytes(bytes)
            })
        };
        let (name_len, data_len) = (number(6), number(8));
        assert_eq!((name_len, data_len), (18, 16));
        let name_padded = (name_len + 3) & !3;
        let mut body = vec![0; name_padded + ((data_len + 3) & !3)];
        peer.read_exact(&mut body).unwrap();
        assert_eq!(&body[..name_len], b"MIT-MAGIC-COOKIE-1");
        assert_eq!(&body[name_padded..name_padded + data_len], &cookie);
        let mut rest = Vec::new();
        peer.read_to_end(&mut rest).unwrap();
        assert!(rest.is_empty(), "failed handshake must not send any grabs");
    });
    let cancel = Arc::new(AtomicBool::new(false));
    assert_timeout(arm_socket(
        client,
        Some(cookie.to_vec()),
        Arc::clone(&cancel),
        Instant::now() + Duration::from_millis(200),
    ));
    peer_thread.join().unwrap();
    assert_eq!(Arc::strong_count(&cancel), 1);
}
