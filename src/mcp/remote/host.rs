//! Persistent bounded multi-client TLS front end and single-task supervisor.
//! The listener owns the broker/process; connection thread teardown cannot
//! release desktop authority. Discovery never starts a task executor.
use super::{broker::Broker, executor::EXIT_GRACE};
use crate::mcp::tcp::{load_token, TcpError};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Includes TLS handshakes, unauthenticated peers and drain/teardown threads.
/// No unbounded accept-thread spawning, even under a slow authentication flood.
pub const MAX_CLIENTS: usize = 16;

pub struct RemoteArgs {
    pub listen: std::net::SocketAddr,
    pub tls_cert: PathBuf,
    pub tls_key: PathBuf,
    pub token_file: PathBuf,
    pub log_file: Option<PathBuf>,
    pub mock_backend: bool,
    pub feedback: crate::feedback::FeedbackConfig,
}

pub fn run(args: RemoteArgs, shutdown_flag: Arc<AtomicBool>) -> Result<(), TcpError> {
    let config = super::tls::server_config(&args.tls_cert, &args.tls_key)
        .map_err(|e| TcpError::Token(format!("tls config: {e}")))?;
    let token = Arc::new(load_token(&args.token_file)?);
    let listener = TcpListener::bind(args.listen)?;
    listener.set_nonblocking(true)?;
    eprintln!("[computer-host] remote TLS listening on {} (up to {MAX_CLIENTS} connections; one lazy task executor)", args.listen);
    let mut broker = Broker::new(args.mock_backend, args.feedback);
    let mut clients: Vec<(Arc<AtomicBool>, std::thread::JoinHandle<()>)> = Vec::new();
    let mut connection_id = 0_u64;
    let mut failure = None;
    while !shutdown_flag.load(Ordering::SeqCst) {
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| broker.tick())).is_err() {
            // The process handle is retained by broker. Panic never means release.
            broker.panic_cleanup();
            failure = Some(TcpError::Token(
                "desktop broker panicked; authority quarantined".into(),
            ));
            break;
        }
        let mut index = 0;
        while index < clients.len() {
            if clients[index].1.is_finished() {
                let (_, thread) = clients.swap_remove(index);
                let _ = thread.join();
            } else {
                index += 1;
            }
        }
        match listener.accept() {
            Ok((sock, _)) => {
                if clients.len() >= MAX_CLIENTS {
                    drop(sock);
                    continue;
                }
                let Some(next) = connection_id.checked_add(1) else {
                    drop(sock);
                    break;
                };
                connection_id = next;
                let alive = Arc::new(AtomicBool::new(true));
                let connection_alive = alive.clone();
                let cfg = config.clone();
                let tok = token.clone();
                let handle = broker.handle.clone();
                let shutdown = shutdown_flag.clone();
                match std::thread::Builder::new()
                    .name(format!("remote-client-{next}"))
                    .spawn(move || {
                        super::connection::serve(
                            sock,
                            cfg,
                            tok,
                            next,
                            handle,
                            connection_alive,
                            shutdown,
                        );
                    }) {
                    Ok(thread) => clients.push((alive, thread)),
                    Err(e) => {
                        failure = Some(TcpError::Io(e));
                        break;
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(5))
            }
            Err(e) => {
                failure = Some(TcpError::Io(e));
                break;
            }
        }
    }
    // Signal all disconnects first, not serial per-client joins; EOF promptly
    // reaches the task's independent cancellation path while responses drain.
    for (alive, _) in &clients {
        broker.handle.disconnect(alive);
    }
    broker.stop();
    let until = Instant::now() + EXIT_GRACE + Duration::from_secs(5);
    let mut shutdown_panicked = false;
    while !broker.idle() && Instant::now() < until {
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| broker.tick())).is_err() {
            shutdown_panicked = true;
            broker.panic_cleanup();
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    for (_, thread) in clients {
        let _ = super::pump::wait_join(thread, Duration::from_secs(5));
    }
    shutdown_result(failure, broker.teardown_is_clean(), shutdown_panicked)
}

/// Reaching the bounded shutdown deadline is not proof of cleanup. Preserve an
/// earlier failure; otherwise require positive authority/process confirmation
/// and explicitly report any panic caught during the shutdown tick loop.
fn shutdown_result(
    failure: Option<TcpError>,
    teardown_clean: bool,
    shutdown_panicked: bool,
) -> Result<(), TcpError> {
    if let Some(error) = failure {
        Err(error)
    } else if shutdown_panicked {
        Err(TcpError::Token(
            "desktop broker panicked during shutdown; cleanup unconfirmed".into(),
        ))
    } else if !teardown_clean {
        Err(TcpError::Token(
            "desktop broker shutdown unconfirmed: executor retained or authority unavailable/quarantined".into(),
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{shutdown_result, TcpError};

    #[test]
    fn shutdown_reports_success_only_with_positive_clean_teardown() {
        assert!(shutdown_result(None, true, false).is_ok());
        assert!(shutdown_result(None, false, false).is_err());
        // A caught shutdown panic is an explicit failure even if a later
        // cleanup path happens to report an otherwise healthy state.
        let panic = shutdown_result(None, true, true).unwrap_err();
        assert!(panic.to_string().contains("panicked during shutdown"));
    }

    #[test]
    fn shutdown_retains_original_failure_instead_of_masking_it() {
        for clean in [false, true] {
            for panicked in [false, true] {
                let earlier = TcpError::Io(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "original listener failure",
                ));
                match shutdown_result(Some(earlier), clean, panicked) {
                    Err(TcpError::Io(error)) => {
                        assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
                        assert_eq!(error.to_string(), "original listener failure");
                    }
                    other => panic!("original failure was replaced: {other:?}"),
                }
            }
        }
    }

    #[test]
    fn feedback_forwarding_is_argv_only_and_contains_no_tls_or_token_fields() {
        let feedback = crate::feedback::FeedbackConfig {
            executable: Some("C:/Program Files/Feedback/renderer.exe".into()),
            accent: Some("#2288aa".into()),
            label: Some("AI control".into()),
        };
        let mut command = std::process::Command::new("owned-child.exe");
        feedback.append_args(&mut command);
        let args: Vec<_> = command
            .get_args()
            .map(|s| s.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            [
                "--desktop-feedback",
                "C:/Program Files/Feedback/renderer.exe",
                "--feedback-accent",
                "#2288aa",
                "--feedback-label",
                "AI control"
            ]
        );
    }
}
