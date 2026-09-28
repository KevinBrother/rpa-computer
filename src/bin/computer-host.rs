//! `computer-host` — MCP server exposing the computer-use Runtime.
//!
//! Default mode is stdio (newline-delimited JSON-RPC MCP). An optional
//! loopback-only, token-authenticated TCP mode exists for interactive
//! Windows test sessions (`--listen 127.0.0.1:PORT --token-file PATH`).
//!
//! stdout carries protocol frames ONLY. All diagnostics go to stderr and
//! never contain tokens or image data.

use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use rpa_computer::mcp::backend_factory::BackendFactory;
use rpa_computer::mcp::hotkey;
use rpa_computer::mcp::lock::{self, DesktopLock};
use rpa_computer::mcp::stdio;
use rpa_computer::mcp::tcp;
use rpa_computer::mcp::worker::Worker;
use rpa_computer::runtime::tool_definitions;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = match Options::parse(&args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("computer-host: {e}");
            eprintln!("try --help");
            return ExitCode::from(2);
        }
    };

    match options.mode {
        Mode::Help => {
            print!("{}", help_text());
            ExitCode::SUCCESS
        }
        Mode::Version => {
            println!("computer-host {VERSION}");
            ExitCode::SUCCESS
        }
        Mode::Describe { mock_backend } => {
            print_describe(mock_backend);
            ExitCode::SUCCESS
        }
        Mode::Serve {
            listen,
            token_file,
            mock_backend,
        } => serve(listen, token_file, mock_backend),
    }
}

enum Mode {
    Help,
    Version,
    Describe {
        mock_backend: bool,
    },
    Serve {
        listen: Option<std::net::SocketAddr>,
        token_file: Option<std::path::PathBuf>,
        mock_backend: bool,
    },
}

struct Options {
    mode: Mode,
}

impl Options {
    fn parse(args: &[String]) -> Result<Self, String> {
        if args.iter().any(|a| a == "--help" || a == "-h") {
            return Ok(Options { mode: Mode::Help });
        }
        if args.iter().any(|a| a == "--version" || a == "-V") {
            return Ok(Options {
                mode: Mode::Version,
            });
        }
        // Explicit diagnostic mock backend: ONLY when the operator passed
        // the flag themselves. Never a fallback from native errors.
        let mock_backend = args.iter().any(|a| a == "--mock-backend");
        if args.iter().any(|a| a == "--describe") {
            return Ok(Options {
                mode: Mode::Describe { mock_backend },
            });
        }

        let mut listen = None;
        let mut token_file = None;
        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--listen" => {
                    let value = args
                        .get(i + 1)
                        .ok_or("--listen requires a 127.0.0.1:PORT argument")?;
                    let addr = tcp::ensure_loopback(value).map_err(|e| e.to_string())?;
                    listen = Some(addr);
                    i += 2;
                }
                "--token-file" => {
                    let value = args
                        .get(i + 1)
                        .ok_or("--token-file requires a PATH argument")?;
                    token_file = Some(std::path::PathBuf::from(value));
                    i += 2;
                }
                "--mock-backend" => {
                    i += 1;
                }
                other => return Err(format!("unknown argument: {other}")),
            }
        }

        match (listen, token_file) {
            (Some(_), None) => Err("--listen requires --token-file".into()),
            (None, Some(_)) => Err("--token-file requires --listen".into()),
            (l, t) => Ok(Options {
                mode: Mode::Serve {
                    listen: l,
                    token_file: t,
                    mock_backend,
                },
            }),
        }
    }
}

fn serve(
    listen: Option<std::net::SocketAddr>,
    token_file: Option<std::path::PathBuf>,
    mock_backend: bool,
) -> ExitCode {
    // 1. Cross-process desktop writer exclusion BEFORE any backend work.
    //    Contention yields a clean diagnostic; no input is ever injected.
    //    In explicit mock mode no real desktop input is possible, so the
    //    real writer lock / hotkey are skipped — and that is said out loud.
    let desktop_lock: Option<DesktopLock> = if mock_backend {
        eprintln!(
            "[computer-host] MOCK BACKEND (--mock-backend): diagnostics only; \
             no real capture, no input injection, no desktop writer lock, no hotkey"
        );
        None
    } else {
        match lock::acquire() {
            Ok(l) => {
                eprintln!(
                    "[computer-host] acquired desktop writer lock at {}",
                    l.path().display()
                );
                Some(l)
            }
            Err(e) => {
                eprintln!("{{\"error\":{{\"code\":\"lease_conflict\",\"message\":\"{e}\"}}}}");
                return ExitCode::from(3);
            }
        }
    };

    // 2. Shared shutdown flag: signals set it; the hotkey watcher sets it too.
    let shutdown_flag = Arc::new(AtomicBool::new(false));
    install_signal_handlers(&shutdown_flag);

    // 3. Runtime worker with the selected backend, constructed on its
    //    thread. Default is the real native desktop; mock is explicit-only.
    let factory = if mock_backend {
        BackendFactory::Mock
    } else {
        BackendFactory::Desktop
    };
    let worker = match Worker::start(factory) {
        Ok(w) => w,
        Err(e) => {
            eprintln!(
                "[computer-host] backend initialization failed: {e}. \
                 Check screen-recording/accessibility permissions."
            );
            return ExitCode::from(4);
        }
    };
    let cancel = worker.cancel_handle();

    // 4. Emergency hotkey: best effort, honestly reported. The hotkey sets
    //    its own flag; a watcher folds that into worker cancel + shutdown.
    //    If arming fails we say so on stderr and in --describe; EOF alone is
    //    NOT represented as a local emergency stop. Mock mode injects no
    //    real input, so no hotkey is armed there (reported, not hidden).
    let hotkey_flag = Arc::new(AtomicBool::new(false));
    let hotkey_guard = if mock_backend {
        None
    } else {
        let guard = hotkey::arm(Arc::clone(&hotkey_flag));
        let hotkey_status = guard.status().describe();
        if guard.status().is_armed() {
            eprintln!("[computer-host] {hotkey_status}");
        } else {
            eprintln!("[computer-host] WARNING: {hotkey_status}");
            eprintln!(
                "[computer-host] without the hotkey, local emergency stop relies on \
                 Ctrl+C (SIGINT) in this terminal or closing the MCP client; remote \
                 CLI Ctrl+C is NOT a local emergency stop"
            );
        }
        Some(guard)
    };
    let _ = &hotkey_guard;

    // Hotkey watcher: fold hotkey presses into cancel + shutdown.
    {
        let shutdown_flag = Arc::clone(&shutdown_flag);
        let cancel = cancel.clone();
        let hotkey_flag = Arc::clone(&hotkey_flag);
        std::thread::Builder::new()
            .name("computer-hotkey-watch".into())
            .spawn(move || loop {
                if hotkey_flag.load(Ordering::SeqCst) {
                    cancel.cancel();
                    shutdown_flag.store(true, Ordering::SeqCst);
                    break;
                }
                if shutdown_flag.load(Ordering::SeqCst) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            })
            .expect("spawn hotkey watcher");
    }

    // 5. Serve. A FATAL transport outcome (worker faulted: an abandoned
    //    native thread may still inject) is a non-zero exit — the caller
    //    must restart the host rather than mistake it for a clean stop.
    let code = match (listen, token_file) {
        (None, None) => {
            eprintln!("[computer-host] serving MCP on stdio");
            match stdio::run(&worker, VERSION, Arc::clone(&shutdown_flag)) {
                stdio::StdioOutcome::Clean => ExitCode::SUCCESS,
                stdio::StdioOutcome::Faulted => {
                    eprintln!(
                        "[computer-host] exiting non-zero: worker faulted (shutdown_unknown)"
                    );
                    ExitCode::from(7)
                }
                stdio::StdioOutcome::WorkerInitFailed(m) => {
                    eprintln!("[computer-host] worker init failed: {m}");
                    ExitCode::from(4)
                }
            }
        }
        (Some(addr), Some(path)) => {
            let token = match tcp::load_token(&path) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("[computer-host] {e}");
                    worker.shutdown();
                    return ExitCode::from(5);
                }
            };
            match tcp::run(&worker, VERSION, addr, token, Arc::clone(&shutdown_flag)) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("[computer-host] tcp transport failed: {e}");
                    ExitCode::from(6)
                }
            }
        }
        (None, Some(_)) | (Some(_), None) => unreachable!("validated in parse"),
    };

    // 6. Cleanup: cancel + runtime shutdown + lock release (Drop). The
    //    writer lock is held for the WHOLE process lifetime — even after a
    //    fault — so an abandoned native thread can never overlap with a new
    //    host acquiring the desktop.
    cancel.cancel();
    worker.shutdown();
    drop(desktop_lock);
    eprintln!("[computer-host] stopped");
    code
}

fn install_signal_handlers(flag: &Arc<AtomicBool>) {
    // Minimal signal handling without external crates: register handlers via
    // libc sigaction that only set the atomic flag. On Windows, Ctrl+C/Close
    // events are handled through SetConsoleCtrlHandler.
    #[cfg(unix)]
    {
        unix_signals::install(flag);
    }
    #[cfg(windows)]
    {
        windows_signals::install(flag);
    }
}

#[cfg(unix)]
mod unix_signals {
    use super::*;
    use std::sync::atomic::AtomicBool;

    static mut FLAG_PTR: *const AtomicBool = std::ptr::null();

    extern "C" fn handler(_sig: i32) {
        // SAFETY: async-signal-safe: only an atomic store.
        unsafe {
            if !FLAG_PTR.is_null() {
                (*FLAG_PTR).store(true, Ordering::SeqCst);
            }
        }
    }

    pub fn install(flag: &Arc<AtomicBool>) {
        // Leak the pointer: the flag lives for the whole process.
        let ptr = Arc::into_raw(Arc::clone(flag));
        unsafe {
            FLAG_PTR = ptr;
            for sig in [libc_sigint(), libc_sigterm(), libc_sighup()] {
                libc_signal(sig, handler as *const () as usize);
            }
        }
    }

    extern "C" {
        #[link_name = "signal"]
        fn libc_signal(signum: i32, handler: usize) -> usize;
    }
    fn libc_sigint() -> i32 {
        2
    }
    fn libc_sigterm() -> i32 {
        15
    }
    #[cfg(target_os = "macos")]
    fn libc_sighup() -> i32 {
        1
    }
    #[cfg(not(target_os = "macos"))]
    fn libc_sighup() -> i32 {
        1
    }
}

#[cfg(windows)]
mod windows_signals {
    use super::*;
    use std::sync::atomic::AtomicBool;

    static mut FLAG_PTR: *const AtomicBool = std::ptr::null();

    unsafe extern "system" fn handler(ctrl_type: u32) -> i32 {
        // CTRL_C_EVENT=0, CTRL_BREAK_EVENT=1, CTRL_CLOSE_EVENT=2
        if ctrl_type <= 2 {
            // SAFETY: atomic store only.
            unsafe {
                if !FLAG_PTR.is_null() {
                    (*FLAG_PTR).store(true, Ordering::SeqCst);
                }
            }
            // For close events the OS will kill us after a short grace
            // period; the flag gives the main loop a chance to clean up.
            std::thread::sleep(std::time::Duration::from_millis(500));
            return 1;
        }
        0
    }

    pub fn install(flag: &Arc<AtomicBool>) {
        #[link(name = "kernel32")]
        extern "system" {
            fn SetConsoleCtrlHandler(
                handler: Option<unsafe extern "system" fn(u32) -> i32>,
                add: i32,
            ) -> i32;
        }
        let ptr = Arc::into_raw(Arc::clone(flag));
        unsafe {
            FLAG_PTR = ptr;
            SetConsoleCtrlHandler(Some(handler), 1);
        }
    }
}

fn help_text() -> String {
    format!(
        "computer-host {VERSION}\n\
         MCP server exposing computer-control tools (screenshot + input injection).\n\
         \n\
         USAGE:\n\
         \x20   computer-host                          Serve MCP over stdio (default)\n\
         \x20   computer-host --listen 127.0.0.1:PORT --token-file PATH\n\
         \x20                                             Serve MCP over authenticated loopback TCP\n\
         \x20                                             (interactive Windows test sessions only)\n\
         \x20   computer-host --describe               Print capabilities and diagnostics (no input)\n\
         \x20   computer-host --mock-backend           DIAGNOSTIC mock backend (no real capture,\n\
         \x20                                             no input, no writer lock, no hotkey);\n\
         \x20                                             combinable with stdio or --listen\n\
         \x20   computer-host --version                Print version\n\
         \x20   computer-host --help                   This help\n\
         \n\
         PROTOCOL:\n\
         \x20   Newline-delimited JSON-RPC MCP ({}). stdout is protocol-only;\n\
         \x20   diagnostics go to stderr and never contain tokens or image data.\n\
         \n\
         SAFETY:\n\
         \x20   One host process per desktop (OS-level writer lock).\n\
         \x20   Emergency stop: native hotkey when armed (see --describe),\n\
         \x20   plus Ctrl+C/SIGTERM and client disconnect (EOF).\n",
        rpa_computer::mcp::jsonrpc::PROTOCOL_VERSION
    )
}

fn print_describe(mock_backend: bool) {
    // Pure diagnostics: no screenshots, no input injection.
    let tools = tool_definitions();
    let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    let hotkey_probe = if mock_backend {
        "disabled (mock backend mode)"
    } else {
        "probed at startup (armed state reported on stderr)"
    };
    let describe = serde_json::json!({
        "name": "computer-host",
        "version": VERSION,
        "backend": if mock_backend { "mock" } else { "desktop" },
        "protocol": {
            "mcp_versions": rpa_computer::mcp::jsonrpc::SUPPORTED_PROTOCOL_VERSIONS,
            "framing": "newline-delimited JSON-RPC over stdio or loopback TCP",
            "max_frame_bytes": rpa_computer::mcp::jsonrpc::MAX_LINE_BYTES,
        },
        "tools": tool_names,
        "transports": {
            "stdio": "default",
            "tcp": "--listen 127.0.0.1:PORT --token-file PATH (loopback only, token first line)"
        },
        "safety": {
            "desktop_writer_lock": if mock_backend {
                "not held (mock backend mode; no real input possible)"
            } else {
                "OS-level exclusive lock (flock / LockFileEx), held for the whole process lifetime"
            },
            "session_lifetime_seconds": rpa_computer::mcp::worker::SESSION_LIFETIME.as_secs(),
            "emergency_hotkey": hotkey_probe,
        },
        "platform": std::env::consts::OS,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&describe).unwrap_or_default()
    );
}
