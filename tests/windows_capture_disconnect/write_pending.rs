//! Stage A: compile this as a CHILD module of the unchanged production pump.
//! The owned build harness appends only a #[path] module declaration to a
//! byte-for-byte copy of pump.rs, making the actual private helper accessible.
//! No sockets, processes, desktop APIs, fake writers, or raised buffer limits.

use super::{write_pending, CHUNK_BYTES};
use crate::tls::TLS_BUFFER_LIMIT;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use rustls::{
    ClientConfig, ClientConnection, Connection, RootCertStore, ServerConfig, ServerConnection,
};
use std::io::{Cursor, ErrorKind, Read, Write};
use std::sync::Arc;

const CA: &[u8] = include_bytes!("../fixtures/remote/good/ca.pem");
const CERT: &[u8] = include_bytes!("../fixtures/remote/good/server.pem");
const KEY: &[u8] = include_bytes!("../fixtures/remote/good/server.key");
const MAX_RECORD_PASSES: usize = 1024;

// Drive REAL TLS records entirely in memory. Drain decrypted data after each
// read so a receive-buffer limit cannot become a different failure mechanism.
fn transfer(from: &mut Connection, to: &mut Connection) -> Vec<u8> {
    let mut wire = Vec::new();
    for pass in 0..MAX_RECORD_PASSES {
        if !from.wants_write() {
            break;
        }
        assert!(
            pass + 1 < MAX_RECORD_PASSES,
            "TLS write setup exceeded bound"
        );
        assert!(from.write_tls(&mut wire).expect("TLS record write") > 0);
    }
    let mut cursor = Cursor::new(wire);
    let mut plaintext = Vec::new();
    let mut buf = [0u8; CHUNK_BYTES];
    for pass in 0..MAX_RECORD_PASSES {
        if cursor.position() as usize == cursor.get_ref().len() {
            return plaintext;
        }
        assert!(
            pass + 1 < MAX_RECORD_PASSES,
            "TLS read setup exceeded bound"
        );
        assert!(to.read_tls(&mut cursor).expect("TLS record read") > 0);
        to.process_new_packets()
            .expect("valid authenticated TLS records");
        loop {
            match to.reader().read(&mut buf) {
                Ok(0) => panic!("unexpected clean close during open-stream regression"),
                Ok(n) => plaintext.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) => panic!("TLS plaintext read: {e}"),
            }
        }
    }
    panic!("TLS transfer exceeded bound");
}

fn established_pair() -> (Connection, Connection) {
    let mut roots = RootCertStore::empty();
    roots
        .add(CertificateDer::from_pem_slice(CA).expect("fixture CA"))
        .unwrap();
    let client_config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let certs = CertificateDer::pem_slice_iter(CERT)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let server_config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(
            certs,
            PrivateKeyDer::from_pem_slice(KEY).expect("fixture private key"),
        )
        .unwrap();
    let mut client = Connection::Client(
        ClientConnection::new(
            Arc::new(client_config),
            ServerName::try_from("localhost").unwrap(),
        )
        .unwrap(),
    );
    let mut server = Connection::Server(ServerConnection::new(Arc::new(server_config)).unwrap());
    client.set_buffer_limit(Some(TLS_BUFFER_LIMIT));
    server.set_buffer_limit(Some(TLS_BUFFER_LIMIT));
    for _ in 0..64 {
        assert!(transfer(&mut client, &mut server).is_empty());
        assert!(transfer(&mut server, &mut client).is_empty());
        if !client.is_handshaking()
            && !server.is_handshaking()
            && !client.wants_write()
            && !server.wants_write()
        {
            return (server, client);
        }
    }
    panic!("in-memory TLS handshake exceeded bound");
}

fn pattern(length: usize, seed: usize) -> Vec<u8> {
    (0..length).map(|i| ((i * 73 + seed) % 251) as u8).collect()
}

#[test]
fn full_tls_buffer_retains_pending_instead_of_disconnect() {
    let (mut server, mut client) = established_pair();
    let chunk = pattern(CHUNK_BYTES, 13);
    let mut queued = Vec::new();
    let mut full = false;
    for _ in 0..(TLS_BUFFER_LIMIT / CHUNK_BYTES + 2) {
        let n = server.writer().write(&chunk).expect("fill real TLS buffer");
        if n == 0 {
            full = true;
            break;
        }
        queued.extend_from_slice(&chunk[..n]);
    }
    assert!(
        full && !queued.is_empty() && server.wants_write(),
        "setup must reach actual rustls backpressure"
    );
    let original = pattern(CHUNK_BYTES, 29);
    let mut pending = original.clone();
    let mut off = 0;
    let result = write_pending(&mut server, &mut pending, &mut off);
    // CURRENT production returns Err("tls writer accepted 0 bytes") here.
    assert_eq!(
        result,
        Ok(false),
        "a full bounded rustls buffer is retryable, not a transport disconnect"
    );
    assert_eq!(
        off, 0,
        "zero accepted bytes must not advance the retained offset"
    );
    assert_eq!(pending, original, "backpressure must not discard plaintext");
    assert_eq!(transfer(&mut server, &mut client), queued);
    assert_eq!(write_pending(&mut server, &mut pending, &mut off), Ok(true));
    assert!(pending.is_empty());
    assert_eq!(off, 0);
    assert_eq!(
        transfer(&mut server, &mut client),
        original,
        "retry delivers exactly once"
    );
}

#[test]
fn partial_tls_write_retains_offset_and_resumes_without_replay() {
    let (mut server, mut client) = established_pair();
    // Record overhead leaves some space, but strictly less than CHUNK_BYTES.
    // Keep the PRODUCTION 4 MiB bound: do not manufacture a tiny test limit.
    let queued = pattern(TLS_BUFFER_LIMIT - CHUNK_BYTES / 2, 41);
    assert_eq!(server.writer().write(&queued).unwrap(), queued.len());
    let original = pattern(CHUNK_BYTES, 53);
    let mut pending = original.clone();
    let mut off = 0;
    let result = write_pending(&mut server, &mut pending, &mut off);
    assert!(
        off > 0 && off < original.len(),
        "setup must accept a strict prefix before reaching buffer capacity; off={off}"
    );
    // CURRENT production advances off, then treats the next Ok(0) as fatal.
    assert_eq!(
        result,
        Ok(false),
        "partial progress followed by zero capacity must remain retryable"
    );
    assert_eq!(
        pending, original,
        "retain the complete buffer plus its consumed offset"
    );
    let accepted = off;
    let mut expected_first_drain = queued;
    expected_first_drain.extend_from_slice(&original[..accepted]);
    assert_eq!(transfer(&mut server, &mut client), expected_first_drain);
    assert_eq!(write_pending(&mut server, &mut pending, &mut off), Ok(true));
    assert!(pending.is_empty());
    assert_eq!(off, 0);
    assert_eq!(
        transfer(&mut server, &mut client),
        original[accepted..],
        "resume only the unsent suffix, never replay the prefix"
    );
}

#[test]
fn writable_tls_buffer_consumes_one_chunk_exactly_once() {
    let (mut server, mut client) = established_pair();
    let expected = pattern(CHUNK_BYTES, 67);
    let mut pending = expected.clone();
    let mut off = 0;
    assert_eq!(write_pending(&mut server, &mut pending, &mut off), Ok(true));
    assert!(pending.is_empty());
    assert_eq!(off, 0);
    assert_eq!(transfer(&mut server, &mut client), expected);
    assert_eq!(write_pending(&mut server, &mut pending, &mut off), Ok(true));
    assert!(
        transfer(&mut server, &mut client).is_empty(),
        "empty retry must not replay output"
    );
}
