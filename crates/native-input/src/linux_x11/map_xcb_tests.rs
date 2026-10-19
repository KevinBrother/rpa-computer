//! Capture bytes emitted by the ACTUAL public xcb serializer over owned local
//! socket pairs. This is a bounded protocol peer, NOT an X server/GUI acceptance
//! fixture. No DISPLAY, Xauth, TCP, input, focus changes, clipboard or external sockets.
use super::*;
use crate::linux_x11::definition::Definition;
use crate::linux_x11::map_request::{target_identity as target, MutationKind, NativeMapRequest};
use std::io::{ErrorKind, Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

const KEY: u8 = 248;
const XKB_OPCODE: u8 = 130;
#[derive(Clone, Copy)]
enum PeerMode {
    Healthy,
    FirstMapBadValue,
    DropAfterMap,
}
fn word(bytes: &[u8], at: usize) -> u16 {
    u16::from_ne_bytes(bytes[at..at + 2].try_into().unwrap())
}
fn put_word(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_ne_bytes());
}
fn put_dword(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_ne_bytes());
}
fn peer(mut stream: UnixStream, mode: PeerMode) -> Vec<Vec<u8>> {
    // Failure bounds, never sleeps used as evidence. Dropping the client ends
    // the peer; each IO also has a finite deadline and packet/iteration cap.
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut hello = [0; 12];
    stream.read_exact(&mut hello).unwrap();
    assert_eq!(
        hello[0],
        if cfg!(target_endian = "little") {
            b'l'
        } else {
            b'B'
        }
    );
    assert_eq!(word(&hello, 2), 11);
    assert_eq!(
        (word(&hello, 6), word(&hello, 8)),
        (0, 0),
        "no credentials in this fixture"
    );
    let mut setup = [0; 40];
    setup[0] = 1; // setup success, fixed32 bytes after the first8
    put_word(&mut setup, 2, 11);
    put_word(&mut setup, 6, 8);
    put_dword(&mut setup, 12, 0x0200_0000);
    put_dword(&mut setup, 16, 0x001f_ffff);
    put_word(&mut setup, 26, u16::MAX);
    setup[32] = 32;
    setup[33] = 32;
    setup[34] = 8;
    setup[35] = 255;
    stream.write_all(&setup).unwrap();
    let mut maps = Vec::new();
    for sequence in 1..=32u16 {
        let mut header = [0; 4];
        match stream.read_exact(&mut header) {
            Err(error) if error.kind() == ErrorKind::UnexpectedEof => return maps,
            result => result.unwrap(),
        }
        let length = usize::from(word(&header, 2)) * 4;
        assert!((4..=8192).contains(&length), "bounded normal request only");
        let mut bytes = vec![0; length];
        bytes[..4].copy_from_slice(&header);
        stream.read_exact(&mut bytes[4..]).unwrap();
        match header[0] {
            98 => {
                // Actual libxcb extension lookup, not a fixture-side opcode override.
                assert!(bytes.len() >= 8);
                let size = usize::from(word(&bytes, 4));
                assert!(size <= bytes.len() - 8);
                assert_eq!(&bytes[8..8 + size], b"XKEYBOARD");
                let mut reply = [0; 32];
                reply[0] = 1;
                put_word(&mut reply, 2, sequence);
                reply[8] = 1;
                reply[9] = XKB_OPCODE;
                stream.write_all(&reply).unwrap();
            }
            XKB_OPCODE => {
                assert_eq!(
                    header[1], 9,
                    "only SetMap, never fake input or other XKB writes"
                );
                maps.push(bytes);
                assert!(maps.len() <= 8);
                if matches!(mode, PeerMode::DropAfterMap) {
                    return maps;
                }
                if matches!(mode, PeerMode::FirstMapBadValue) && maps.len() == 1 {
                    let mut error = [0; 32];
                    error[1] = 2; // core BadValue, checked request's own sequence
                    put_word(&mut error, 2, sequence);
                    put_word(&mut error, 8, 9);
                    error[10] = XKB_OPCODE;
                    stream.write_all(&error).unwrap();
                }
            }
            43 => {
                // libxcb check_request synchronization, not an application ack.
                assert_eq!(length, 4);
                let mut reply = [0; 32];
                reply[0] = 1;
                put_word(&mut reply, 2, sequence);
                stream.write_all(&reply).unwrap();
            }
            opcode => panic!("unexpected protocol-peer request {opcode}"),
        }
    }
    panic!("protocol-peer request budget exhausted");
}
fn capture(mode: PeerMode, body: impl FnOnce(&Connection)) -> Vec<Vec<u8>> {
    let (client, server) = UnixStream::pair().unwrap();
    let join = std::thread::spawn(move || peer(server, mode));
    // Public ownership-aware API, NOT the deprecated raw-FD constructor. This
    // owned fixture connection is separate from the product's borrowed Display.
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let connection = Connection::connect_with_fd(client.into(), None).unwrap();
        body(&connection);
        // Owned connection drops on success AND unwinding, closing the peer.
    }));
    let packets = join.join();
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
    let packets = packets.unwrap();
    packets
}
fn request(kind: MutationKind) -> NativeMapRequest {
    let inventory = target::Inventory {
        master: 3,
        epoch: 1,
        descriptors: [3, 5]
            .map(|id| target::Descriptor {
                device_id: id,
                range: (8, 255),
            })
            .to_vec(),
        devices: [3, 5]
            .map(|id| target::Device {
                id,
                kind: if id == 3 { 2 } else { 4 },
                attachment: if id == 3 { 2 } else { 3 },
                enabled: true,
                name: format!("wire-fixture-{id}").into_bytes(),
                key_sources: vec![id],
                marker: if id == 3 {
                    target::Marker::NotQueried
                } else {
                    target::Marker::Reply {
                        actual_type: 19,
                        format: 8,
                        items: 1,
                        bytes_after: 0,
                        first_byte: Some(1),
                    }
                },
            })
            .to_vec(),
    };
    target::request(kind, &inventory, KEY).unwrap()
}
fn issue(connection: &Connection, kind: MutationKind, payload: Payload<'_>) -> Result<()> {
    let native = request(kind);
    let prepared = super::super::prepare(&native, payload)?;
    // The SAME send/public constructor/checked classifier called by write().
    send(connection, &prepared)
}
fn definition() -> Definition {
    let mut value = Definition::for_symbol(0x0100_4e2d);
    value.repeat = Some(false);
    value
}
fn assert_header(bytes: &[u8], full: bool, count: usize, explicit: u8) {
    assert_eq!((bytes[0], bytes[1]), (XKB_OPCODE, 9));
    assert_eq!(usize::from(word(bytes, 2)) * 4, bytes.len());
    assert_eq!(
        word(bytes, 4),
        5,
        "explicit verified XTEST slave, never master/core alias"
    );
    assert_eq!(word(bytes, 6), if full { 0xbe } else { 8 });
    assert_eq!(
        word(bytes, 8),
        0,
        "no recompute or resize types in ACTUAL wire"
    );
    assert_eq!(&bytes[10..14], &[8, 255, 0, 0]);
    assert_eq!(&bytes[14..16], if full { &[KEY, 1] } else { &[0, 0] });
    assert_eq!(usize::from(word(bytes, 16)), count);
    assert_eq!(&bytes[18..20], if full { &[KEY, 1] } else { &[0, 0] });
    assert_eq!(word(bytes, 20), 0);
    assert_eq!(&bytes[22..25], if full { &[KEY, 1, 0] } else { &[0, 0, 0] });
    assert_eq!(&bytes[25..28], &[KEY, 1, u8::from(explicit != 0)]);
    assert_eq!(&bytes[28..31], if full { &[KEY, 1, 0] } else { &[0, 0, 0] });
    assert_eq!(&bytes[31..34], if full { &[KEY, 1, 0] } else { &[0, 0, 0] });
    assert_eq!(word(bytes, 34), 0);
}
fn assert_full(bytes: &[u8], value: &Definition) {
    assert_header(bytes, true, value.symbols.len(), value.explicit);
    assert_eq!(&bytes[36..40], &value.types);
    assert_eq!((bytes[40], bytes[41]), (value.group_info, value.width));
    assert_eq!(usize::from(word(bytes, 42)), value.symbols.len());
    let end = 44 + value.symbols.len() * 4;
    for (raw, expected) in bytes[44..end].chunks_exact(4).zip(&value.symbols) {
        assert_eq!(
            u32::from_ne_bytes(raw.try_into().unwrap()) as u64,
            *expected as u64
        );
    }
    // Exactly one zero action-count byte plus three alignment bytes; no action,
    // behavior/modmap/vmodmap element exists. Their present bits still exist.
    assert_eq!(&bytes[end..end + 4], &[0; 4]);
    let tail = if value.explicit == 0 {
        &[][..]
    } else {
        &[KEY, value.explicit, 0, 0][..]
    };
    assert_eq!(&bytes[end + 4..], tail);
}
fn assert_explicit(bytes: &[u8], explicit: u8) {
    assert_header(bytes, false, 0, explicit);
    let tail = if explicit == 0 {
        &[][..]
    } else {
        &[KEY, explicit, 0, 0][..]
    };
    assert_eq!(&bytes[36..], tail);
}

#[test]
fn public_serializer_three_phases_use_exact_counts_order_flags_and_padding() {
    let installed = definition();
    let mut original = definition();
    original.types[0] = 1;
    original.symbols = vec![0, 0];
    original.width = 2;
    original.explicit = 0;
    let guarded = original.guarded_restoration();
    let packets = capture(PeerMode::Healthy, |connection| {
        issue(connection, MutationKind::Install, Payload::Full(&installed)).unwrap();
        issue(
            connection,
            MutationKind::RestoreFull,
            Payload::Full(&guarded),
        )
        .unwrap();
        issue(
            connection,
            MutationKind::RestoreExplicit,
            Payload::Explicit(original.explicit),
        )
        .unwrap();
    });
    assert_eq!(
        packets.iter().map(Vec::len).collect::<Vec<_>>(),
        [56, 60, 36]
    );
    assert_full(&packets[0], &installed);
    assert_full(&packets[1], &guarded);
    assert_explicit(&packets[2], 0);
}
#[test]
fn public_serializer_empty_symbols_and_zero_or_nonzero_explicit_lists() {
    let mut empty = definition();
    empty.group_info = 0;
    empty.width = 0;
    empty.symbols.clear();
    empty.explicit = 0;
    let guarded = empty.guarded_restoration();
    let packets = capture(PeerMode::Healthy, |connection| {
        issue(
            connection,
            MutationKind::RestoreFull,
            Payload::Full(&guarded),
        )
        .unwrap();
        issue(connection, MutationKind::RestoreFull, Payload::Full(&empty)).unwrap();
        issue(
            connection,
            MutationKind::RestoreExplicit,
            Payload::Explicit(0x7f),
        )
        .unwrap();
        issue(
            connection,
            MutationKind::RestoreExplicit,
            Payload::Explicit(0),
        )
        .unwrap();
    });
    assert_eq!(
        packets.iter().map(Vec::len).collect::<Vec<_>>(),
        [52, 48, 40, 36]
    );
    assert_full(&packets[0], &guarded);
    assert_full(&packets[1], &empty);
    assert_explicit(&packets[2], 0x7f);
    assert_explicit(&packets[3], 0);
}
#[test]
fn public_serializer_bounded_maximum_group_width_count_is_not_truncated() {
    let mut value = definition();
    value.group_info = 4;
    value.width = 255;
    value.symbols = vec![0; 4 * 255];
    let packets = capture(PeerMode::Healthy, |connection| {
        issue(connection, MutationKind::RestoreFull, Payload::Full(&value)).unwrap();
    });
    assert_eq!(packets.len(), 1);
    assert_eq!(packets[0].len(), 4132);
    assert_full(&packets[0], &value);
}
#[test]
fn checked_public_writer_returns_protocol_error_and_can_use_healthy_connection_again() {
    let value = definition();
    let packets = capture(PeerMode::FirstMapBadValue, |connection| {
        let error = issue(connection, MutationKind::Install, Payload::Full(&value)).unwrap_err();
        assert_eq!(error.code, "input_failed");
        connection.has_error().unwrap();
        issue(
            connection,
            MutationKind::RestoreExplicit,
            Payload::Explicit(0),
        )
        .unwrap();
    });
    assert_eq!(packets.len(), 2);
}
#[test]
fn checked_public_writer_does_not_acknowledge_transport_loss_after_packet() {
    let value = definition();
    let packets = capture(PeerMode::DropAfterMap, |connection| {
        let error = issue(connection, MutationKind::Install, Payload::Full(&value)).unwrap_err();
        assert_eq!(error.code, "input_failed");
        assert!(connection.has_error().is_err());
    });
    assert_eq!(
        packets.len(),
        1,
        "packet was submitted before transport was lost"
    );
}
