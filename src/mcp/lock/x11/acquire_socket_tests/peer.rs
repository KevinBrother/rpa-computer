//! Minimal core-X11 wire peer, not a desktop/input server. The endpoint uses
//! XCB's real DISPLAY transport path; no environment variables are mutated.
use std::io::{self, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

pub(super) struct Listener {
    socket: UnixListener,
    path: PathBuf,
    pub display: String,
}
impl Listener {
    pub fn bind() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        // Independent CC runs this in an isolated Linux container. Never
        // unlink an existing endpoint or change the shared directory's mode.
        std::fs::create_dir_all("/tmp/.X11-unix").unwrap();
        for _ in 0..100 {
            let number =
                100_000 + std::process::id() % 1_000_000 + NEXT.fetch_add(1, Ordering::SeqCst);
            let path = PathBuf::from(format!("/tmp/.X11-unix/X{number}"));
            match UnixListener::bind(&path) {
                Ok(socket) => {
                    socket.set_nonblocking(true).unwrap();
                    return Self {
                        socket,
                        path,
                        display: format!("unix/:{number}"),
                    };
                }
                Err(e) if e.kind() == io::ErrorKind::AddrInUse => continue,
                Err(e) => panic!("isolated Unix endpoint fixture: {e}"),
            }
        }
        panic!("could not allocate unique fixture endpoint");
    }
    pub fn accept(&self) -> Peer {
        let until = Instant::now() + Duration::from_secs(2);
        loop {
            match self.socket.accept() {
                Ok((socket, _)) => return Peer::handshake(socket),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock && Instant::now() < until => {
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(e) => panic!("actual Connection::connect did not reach fixture: {e}"),
            }
        }
    }
}
impl Drop for Listener {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub(super) struct Peer {
    pub socket: UnixStream,
    order: u8,
    sequence: u16,
}
impl Peer {
    fn handshake(mut socket: UnixStream) -> Self {
        socket
            .set_read_timeout(Some(Duration::from_millis(300)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_millis(300)))
            .unwrap();
        let mut header = [0u8; 12];
        socket
            .read_exact(&mut header)
            .expect("real XCB setup header");
        let order = header[0];
        assert!(matches!(order, b'l' | b'B'));
        let read16 = |offset| {
            let bytes = [header[offset], header[offset + 1]];
            if order == b'l' {
                u16::from_le_bytes(bytes)
            } else {
                u16::from_be_bytes(bytes)
            }
        };
        // Consume but never log/retain auth bytes. This fixture does not depend
        // on any real desktop authorization; isolated CC should supply none.
        let padded = |n: u16| (n as usize + 3) & !3;
        let mut auth = vec![0; padded(read16(6)) + padded(read16(8))];
        socket.read_exact(&mut auth).unwrap();
        auth.fill(0);
        Self {
            socket,
            order,
            sequence: 0,
        }
    }
    pub fn setup(&mut self) -> io::Result<()> {
        // Successful protocol 11 setup: one root, no vendor/pixmap/visual data.
        let mut bytes = vec![0u8; 80];
        bytes[0] = 1;
        self.put16(&mut bytes, 2, 11);
        self.put16(&mut bytes, 6, 18);
        self.put32(&mut bytes, 12, 0x200000);
        self.put32(&mut bytes, 16, 0x1fffff);
        self.put16(&mut bytes, 26, 65535);
        bytes[28] = 1;
        bytes[34] = 8;
        bytes[35] = 255;
        self.put32(&mut bytes, 40, 0x100);
        self.put16(&mut bytes, 60, 800);
        self.put16(&mut bytes, 62, 600);
        bytes[78] = 24;
        self.socket.write_all(&bytes)
    }
    pub fn request(&mut self) -> io::Result<Option<Vec<u8>>> {
        let mut header = [0u8; 4];
        match self.socket.read(&mut header[..1])? {
            0 => return Ok(None),
            1 => {}
            _ => unreachable!(),
        }
        self.socket.read_exact(&mut header[1..])?;
        let words = if self.order == b'l' {
            u16::from_le_bytes([header[2], header[3]])
        } else {
            u16::from_be_bytes([header[2], header[3]])
        } as usize;
        if words == 0 || words > 4096 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unexpected fixture request length",
            ));
        }
        let mut request = vec![0u8; words * 4];
        request[..4].copy_from_slice(&header);
        self.socket.read_exact(&mut request[4..])?;
        self.sequence = self.sequence.wrapping_add(1);
        Ok(Some(request))
    }
    pub fn reply_value(&mut self, value: u32) -> io::Result<()> {
        let mut reply = [0u8; 32];
        reply[0] = 1;
        self.put16(&mut reply, 2, self.sequence);
        self.put32(&mut reply, 8, value);
        self.socket.write_all(&reply)
    }
    pub fn read32(&self, bytes: &[u8], offset: usize) -> u32 {
        let bytes: [u8; 4] = bytes[offset..offset + 4].try_into().unwrap();
        if self.order == b'l' {
            u32::from_le_bytes(bytes)
        } else {
            u32::from_be_bytes(bytes)
        }
    }
    fn put16(&self, bytes: &mut [u8], offset: usize, value: u16) {
        bytes[offset..offset + 2].copy_from_slice(&if self.order == b'l' {
            value.to_le_bytes()
        } else {
            value.to_be_bytes()
        });
    }
    fn put32(&self, bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&if self.order == b'l' {
            value.to_le_bytes()
        } else {
            value.to_be_bytes()
        });
    }
}
