//! Only the parent integration executable embeds public throwaway TLS fixtures.
//! The remote/stdio fixture executable contains no embedded TLS material.
use super::common;
use std::path::{Path, PathBuf};
pub struct Assets {
    pub ca: PathBuf,
    pub cert: PathBuf,
    pub key: PathBuf,
    pub host_token: PathBuf,
    pub token: Vec<u8>,
}
impl Assets {
    pub fn create(dir: &Path) -> Self {
        std::fs::create_dir(dir).unwrap();
        for (name, bytes) in [
            (
                "ca.pem",
                include_bytes!("../fixtures/remote/good/ca.pem").as_slice(),
            ),
            (
                "server.pem",
                include_bytes!("../fixtures/remote/good/server.pem").as_slice(),
            ),
            (
                "server.key",
                include_bytes!("../fixtures/remote/good/server.key").as_slice(),
            ),
            (
                "host.token",
                include_bytes!("../fixtures/remote/good/host.token").as_slice(),
            ),
        ] {
            common::write_new(&dir.join(name), bytes).unwrap();
        }
        let token = include_str!("../fixtures/remote/good/client.token")
            .trim()
            .as_bytes()
            .to_vec();
        assert_eq!(
            token,
            include_str!("../fixtures/remote/good/host.token")
                .trim()
                .as_bytes()
        );
        Self {
            ca: dir.join("ca.pem"),
            cert: dir.join("server.pem"),
            key: dir.join("server.key"),
            host_token: dir.join("host.token"),
            token,
        }
    }
}
