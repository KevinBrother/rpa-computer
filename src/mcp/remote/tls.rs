//! Direct TLS transport: TLS configuration shared by host and client.
//!
//! Security properties:
//! - TLS only. There is NO plaintext/insecure mode anywhere in this module.
//! - Host: server certificate + private key from PEM files (full chain in
//!   the cert file). Client: exactly the operator-supplied private CA as the
//!   trust root; server name is verified by rustls (DNS SAN).
//! - rustls 0.23 with the ring provider; TLS 1.2 + 1.3.
//! - TLS record buffering is explicitly bounded via `set_buffer_limit` so a
//!   stalled or hostile peer cannot grow memory without limit.

use std::path::Path;
use std::sync::Arc;

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use rustls::{ClientConfig, RootCertStore, ServerConfig};

/// Bound on rustls' internal plaintext/deframer buffering per connection.
pub const TLS_BUFFER_LIMIT: usize = 4 * 1024 * 1024;

#[derive(Debug)]
pub struct TlsConfigError(String);

impl std::fmt::Display for TlsConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for TlsConfigError {}

fn err<T>(m: impl Into<String>) -> Result<T, TlsConfigError> {
    Err(TlsConfigError(m.into()))
}

/// Build the host-side server config from PEM cert chain + private key.
pub fn server_config(
    cert_path: &Path,
    key_path: &Path,
) -> Result<Arc<ServerConfig>, TlsConfigError> {
    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_file_iter(cert_path)
        .map_err(|e| TlsConfigError(format!("cannot read TLS cert {}: {e}", cert_path.display())))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| TlsConfigError(format!("invalid TLS cert {}: {e}", cert_path.display())))?;
    if certs.is_empty() {
        return err(format!("no certificates found in {}", cert_path.display()));
    }
    let key = PrivateKeyDer::from_pem_file(key_path)
        .map_err(|e| TlsConfigError(format!("cannot read TLS key {}: {e}", key_path.display())))?;
    let mut config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| TlsConfigError(format!("invalid TLS cert/key: {e}")))?;
    config.alpn_protocols = vec![b"mcp-jsonrpc".to_vec()];
    Ok(Arc::new(config))
}

/// Build the client-side config trusting ONLY the given private CA file.
pub fn client_config(ca_path: &Path) -> Result<Arc<ClientConfig>, TlsConfigError> {
    let mut roots = RootCertStore::empty();
    let cas: Vec<CertificateDer<'static>> = CertificateDer::pem_file_iter(ca_path)
        .map_err(|e| TlsConfigError(format!("cannot read CA cert {}: {e}", ca_path.display())))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| TlsConfigError(format!("invalid CA cert {}: {e}", ca_path.display())))?;
    if cas.is_empty() {
        return err(format!("no CA certificates found in {}", ca_path.display()));
    }
    for ca in cas {
        roots
            .add(ca)
            .map_err(|e| TlsConfigError(format!("unusable CA certificate: {e}")))?;
    }
    let mut config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    config.alpn_protocols = vec![b"mcp-jsonrpc".to_vec()];
    Ok(Arc::new(config))
}

/// Parse a server name for certificate verification. DNS names only: for
/// raw IP `--connect` targets the operator MUST pass `--server-name`.
pub fn parse_server_name(name: &str) -> Result<ServerName<'static>, TlsConfigError> {
    if name.parse::<std::net::IpAddr>().is_ok() {
        return err(format!(
            "invalid --server-name {name:?}: an IP address is not a certificate \
             identity; pass the DNS name from the server certificate"
        ));
    }
    match ServerName::try_from(name.to_string()) {
        Ok(ServerName::DnsName(_)) => {
            ServerName::try_from(name.to_string()).map_err(|_| TlsConfigError("unreachable".into()))
        }
        _ => err(format!(
            "invalid --server-name {name:?}: must be a DNS name present in the \
             server certificate (pass --server-name when connecting by IP)"
        )),
    }
}

#[cfg(test)]
#[path = "../../../tests/support/remote_assets.rs"]
mod remote_assets;

#[cfg(test)]
mod tests {
    use super::*;

    use super::remote_assets::Fixtures;

    #[test]
    fn server_config_loads_fixture() {
        let assets = Fixtures::new();
        let c = server_config(
            &assets.path("good/server.pem"),
            &assets.path("good/server.key"),
        );
        assert!(c.is_ok(), "{c:?}");
    }

    #[test]
    fn server_config_rejects_mismatched_key() {
        let assets = Fixtures::new();
        let good_cert = assets.path("good/server.pem");
        let good_key = assets.path("good/server.key");
        let wrong_cert = assets.path("wrong/server.pem");
        let wrong_key = assets.path("wrong/server.key");
        // Both pairs must independently parse and match. Missing files or invalid
        // PEM must never turn this mismatch regression into a false positive.
        server_config(&good_cert, &good_key).expect("valid good fixture pair");
        server_config(&wrong_cert, &wrong_key).expect("valid wrong fixture pair");
        assert_ne!(
            std::fs::read(&good_cert).unwrap(),
            std::fs::read(&wrong_cert).unwrap()
        );
        assert_ne!(
            std::fs::read(&good_key).unwrap(),
            std::fs::read(&wrong_key).unwrap()
        );
        let error = server_config(&good_cert, &wrong_key)
            .unwrap_err()
            .to_string();
        assert!(error.starts_with("invalid TLS cert/key:"), "{error}");
        assert!(!error.contains("cannot read"), "{error}");
        assert!(
            error.contains("KeyMismatch"),
            "expected rustls key mismatch: {error}"
        );
    }

    #[test]
    fn client_config_loads_fixture_ca() {
        let assets = Fixtures::new();
        assert!(client_config(&assets.path("good/ca.pem")).is_ok());
    }

    #[test]
    fn client_config_rejects_garbage() {
        let assets = Fixtures::new();
        let p = assets.create_file("bad-ca.pem", b"not a pem\n");
        assert!(client_config(&p).is_err());
    }

    #[test]
    fn server_name_validation() {
        assert!(parse_server_name("localhost").is_ok());
        assert!(parse_server_name("host.example.com").is_ok());
        assert!(parse_server_name("bad name with spaces").is_err());
        // Bare IPs are rejected: --server-name is required for IP connects.
        assert!(parse_server_name("127.0.0.1").is_err());
    }
}
