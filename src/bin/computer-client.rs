//! `computer-client` — local stdio MCP endpoint bridging to a remote
//! TLS `computer-host --remote-listen`.
//!
//! stdin/stdout carry MCP JSON-RPC frames only. TLS is mandatory (there is
//! NO --insecure mode); the token is read from a FILE, sent only after the
//! TLS handshake, and never logged. Failures exit non-zero with a stderr
//! diagnostic; a truncated TLS session is never a clean exit.

use std::process::ExitCode;

use rpa_computer::mcp::remote::client::{self, ClientArgs};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse(&args) {
        Ok(Parsed::Run(a)) => ExitCode::from(client::run(a)),
        Ok(Parsed::Help) => {
            print!("{}", help_text());
            ExitCode::SUCCESS
        }
        Ok(Parsed::Version) => {
            println!("computer-client {VERSION}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("computer-client: {e}");
            eprintln!("try --help");
            ExitCode::from(2)
        }
    }
}

enum Parsed {
    Run(ClientArgs),
    Help,
    Version,
}

fn parse(args: &[String]) -> Result<Parsed, String> {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        return Ok(Parsed::Help);
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        return Ok(Parsed::Version);
    }
    let mut connect = None;
    let mut ca_cert = None;
    let mut server_name = None;
    let mut token_file = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--connect" => {
                connect = Some(
                    args.get(i + 1)
                        .ok_or("--connect requires a HOST:PORT argument")?
                        .clone(),
                );
                i += 2;
            }
            "--ca-cert" => {
                ca_cert = Some(std::path::PathBuf::from(
                    args.get(i + 1)
                        .ok_or("--ca-cert requires a PATH argument")?,
                ));
                i += 2;
            }
            "--server-name" => {
                server_name = Some(
                    args.get(i + 1)
                        .ok_or("--server-name requires a DNS name argument")?
                        .clone(),
                );
                i += 2;
            }
            "--token-file" => {
                token_file = Some(std::path::PathBuf::from(
                    args.get(i + 1)
                        .ok_or("--token-file requires a PATH argument")?,
                ));
                i += 2;
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    match (connect, ca_cert, token_file) {
        (None, _, _) => Err("--connect is required".into()),
        (Some(_), None, _) => Err("--connect requires --ca-cert".into()),
        (Some(_), Some(_), None) => Err("--connect requires --token-file".into()),
        (Some(connect), Some(ca_cert), Some(token_file)) => Ok(Parsed::Run(ClientArgs {
            connect,
            ca_cert,
            server_name,
            token_file,
        })),
    }
}

fn help_text() -> String {
    format!(
        "computer-client {VERSION}\n\
         stdio MCP endpoint bridging to a remote TLS computer-host.\n\
         \n\
         USAGE:\n\
         \x20   computer-client --connect HOST:PORT --ca-cert ca.pem \\\n\
         \n\
         \x20       [--server-name DNS-NAME] --token-file client.token\n\
         \x20   computer-client --version\n\
         \x20   computer-client --help\n\
         \n\
         NOTES:\n\
         \x20   TLS is mandatory; there is no insecure mode. --server-name defaults\n\
         \x20   to the host part of --connect and MUST be a DNS name present in the\n\
         \x20   server certificate (required when connecting by IP). The token is\n\
         \x20   read from a file and never appears in argv or logs. stdin/stdout\n\
         \x20   carry MCP JSON-RPC frames only; diagnostics go to stderr.\n"
    )
}
