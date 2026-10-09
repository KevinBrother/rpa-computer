//! Isolated test executable: production remote supervisor re-execs THIS binary.
//! The sole child route is --mock-backend -> Test Backend, never product Host.
#[cfg(windows)]
#[path = "windows_stdio_eof_fixture/backend.rs"]
mod backend;
#[cfg(windows)]
#[path = "windows_tcp_disconnect_fixture/common.rs"]
mod common;
#[cfg(windows)]
#[allow(dead_code)]
#[path = "windows_stdio_eof_fixture/identity.rs"]
mod identity;
#[cfg(windows)]
#[path = "windows_tcp_disconnect_fixture/supervisor.rs"]
mod supervisor;
#[cfg(windows)]
#[path = "windows_tcp_disconnect_fixture/worker.rs"]
mod worker;
#[cfg(windows)]
fn main() -> std::process::ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let result = if args.len() == 1 && args[0] == "--mock-backend" {
        worker::run()
    } else if args.len() == 4 && args[0] == "--tcp-test-supervisor" {
        supervisor::run(&args[1..])
    } else {
        Err(
            "only --mock-backend OR --tcp-test-supervisor <cert> <key> <token-file> are allowed"
                .into(),
        )
    };
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("TCP_FIXTURE_ERROR {e}");
            std::process::ExitCode::from(70)
        }
    }
}
#[cfg(not(windows))]
fn main() {
    panic!("Windows-only test fixture");
}
