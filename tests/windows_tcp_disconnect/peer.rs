//! Real nonblocking TLS socket, absolute bounds; no background socket readers.
use rpa_computer::mcp::remote::tls;
use rustls::ClientConnection;
use serde_json::{json, Value};
use std::{
    io::{self, Read, Write},
    net::{Shutdown, SocketAddr, TcpStream},
    path::Path,
    time::{Duration, Instant},
};
const BOUND: Duration = Duration::from_secs(5);
pub struct Peer {
    pub conn: ClientConnection,
    pub socket: TcpStream,
    plain: Vec<u8>,
    pub received: Vec<Value>,
    pub peer_closed: bool,
    pub socket_eof: bool,
    total: usize,
}
impl Peer {
    pub fn connect(addr: SocketAddr, ca: &Path) -> io::Result<Self> {
        if addr.ip() != std::net::Ipv4Addr::LOCALHOST {
            return Err(io::Error::other("non-loopback peer forbidden"));
        }
        let config = tls::client_config(ca).map_err(io::Error::other)?;
        let conn = ClientConnection::new(
            config,
            tls::parse_server_name("localhost").map_err(io::Error::other)?,
        )
        .map_err(io::Error::other)?;
        let socket = TcpStream::connect_timeout(&addr, Duration::from_secs(2))?;
        socket.set_nonblocking(true)?;
        let mut p = Self {
            conn,
            socket,
            plain: vec![],
            received: vec![],
            peer_closed: false,
            socket_eof: false,
            total: 0,
        };
        let deadline = Instant::now() + BOUND;
        while p.conn.is_handshaking() || p.conn.wants_write() {
            p.drive()?;
            Self::pause(deadline)?;
        }
        Ok(p)
    }
    fn pause(deadline: Instant) -> io::Result<()> {
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "absolute TLS I/O bound",
            ));
        }
        std::thread::park_timeout(Duration::from_millis(2));
        Ok(())
    }
    fn flush_once(&mut self) -> io::Result<()> {
        for _ in 0..64 {
            if !self.conn.wants_write() {
                break;
            }
            match self.conn.write_tls(&mut self.socket) {
                Ok(0) => return Err(io::Error::new(io::ErrorKind::WriteZero, "TLS write zero")),
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
    pub fn flush(&mut self) -> io::Result<()> {
        let until = Instant::now() + BOUND;
        while self.conn.wants_write() {
            self.flush_once()?;
            if self.conn.wants_write() {
                Self::pause(until)?;
            }
        }
        Ok(())
    }
    pub fn drive(&mut self) -> io::Result<()> {
        self.flush_once()?;
        if !self.socket_eof {
            match self.conn.read_tls(&mut self.socket) {
                Ok(0) => {
                    self.socket_eof = true;
                    if !self.peer_closed {
                        return Err(io::Error::new(
                            io::ErrorKind::UnexpectedEof,
                            "peer socket EOF without TLS close_notify",
                        ));
                    }
                }
                Ok(_) => {
                    let state = self
                        .conn
                        .process_new_packets()
                        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                    self.peer_closed |= state.peer_has_closed();
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(e),
            }
        }
        let mut buf = [0u8; 8192];
        loop {
            match self.conn.reader().read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    self.total += n;
                    if self.total > 1024 * 1024 {
                        return Err(io::Error::other("TLS plaintext cap"));
                    }
                    self.plain.extend_from_slice(&buf[..n]);
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e),
            }
        }
        self.flush_once()
    }
    pub fn send_bytes(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.conn.writer().write_all(bytes)?;
        self.flush()
    }
    pub fn authenticate(&mut self, token: &[u8]) -> io::Result<()> {
        // NEVER persist/print authentication bytes.
        self.send_bytes(token)?;
        self.send_bytes(b"\n")?;
        if self.line()? != b"AUTH_OK\n" {
            return Err(io::Error::other("missing AUTH_OK"));
        }
        Ok(())
    }
    fn line(&mut self) -> io::Result<Vec<u8>> {
        let until = Instant::now() + BOUND;
        loop {
            if let Some(n) = self.plain.iter().position(|b| *b == b'\n') {
                return Ok(self.plain.drain(..=n).collect());
            }
            if self.peer_closed || self.socket_eof {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "no complete line",
                ));
            }
            self.drive()?;
            Self::pause(until)?;
        }
    }
    pub fn send(&mut self, value: Value) {
        let mut b = serde_json::to_vec(&value).unwrap();
        b.push(b'\n');
        self.send_bytes(&b).unwrap();
    }
    pub fn request(&mut self, id: u64, method: &str, params: Value) -> Value {
        self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}));
        let frame: Value = serde_json::from_slice(&self.line().unwrap()).unwrap();
        assert_eq!(frame["id"], id);
        assert_eq!(frame["jsonrpc"], "2.0");
        assert!(frame.get("error").is_none(), "{frame}");
        self.received.push(frame.clone());
        frame["result"].clone()
    }
    pub fn close_notify(&mut self) -> io::Result<()> {
        self.conn.send_close_notify();
        self.flush()
    }
    pub fn raw_cut(self) -> io::Result<()> {
        self.socket.shutdown(Shutdown::Both)?;
        drop(self);
        Ok(())
    }
    pub fn drain_tail(&mut self) -> io::Result<()> {
        self.drive()?;
        while let Some(n) = self.plain.iter().position(|b| *b == b'\n') {
            let line: Vec<_> = self.plain.drain(..=n).collect();
            let v: Value = serde_json::from_slice(&line).map_err(io::Error::other)?;
            if v["jsonrpc"] != "2.0" {
                return Err(io::Error::other("non JSON-RPC socket output"));
            }
            self.received.push(v);
        }
        Ok(())
    }
    pub fn tail_complete(&self) -> bool {
        self.plain.is_empty()
    }
}
pub fn data(result: Value) -> Value {
    assert_eq!(result["isError"], false, "{result}");
    serde_json::from_str(
        result["content"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["type"] == "text")
            .unwrap()["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap()
}
