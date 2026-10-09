use super::{common, identity};
use rpa_computer::{
    feedback::FeedbackConfig,
    mcp::remote::host::{self, RemoteArgs},
};
use serde_json::json;
use std::{
    net::{Ipv4Addr, TcpListener},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::Duration,
};
pub fn run(args: &[std::ffi::OsString]) -> Result<(), Box<dyn std::error::Error>> {
    let (dir, nonce) = common::context()?;
    let mut watchdog = common::Watchdog::start(dir.join("supervisor-watchdog.json"));
    let id = identity::current_identity()?;
    let exe = std::env::current_exe()?.canonicalize()?;
    let sha = identity::executable_sha256(&exe)?;
    // Child inherits only owned diagnostics/identity, never TLS key/token via env/argv.
    std::env::set_var("RPA_TCP_SUPERVISOR_ID", serde_json::to_string(&id)?);
    let reservation = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let addr = reservation.local_addr()?;
    if addr.ip() != Ipv4Addr::LOCALHOST || addr.port() == 0 {
        return Err("not exact loopback ephemeral address".into());
    }
    common::publish(
        &dir.join("supervisor.json"),
        &json!({"nonce":nonce,"identity":id,"exe":exe,"sha256":sha,"version":common::version(),"build_id":common::BUILD_ID,"listen":addr.to_string(),"feedback_enabled":false}),
    )?;
    let shutdown = Arc::new(AtomicBool::new(false));
    let flag = shutdown.clone();
    let control_dir = dir.clone();
    let control_nonce = nonce.clone();
    let (done_tx, done_rx) = mpsc::channel();
    let control = std::thread::spawn(move || -> Result<(), String> {
        loop {
            match done_rx.recv_timeout(Duration::from_millis(10)) {
                Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            let path = control_dir.join("stop.request.json");
            if path.exists() {
                let request = common::read(&path).map_err(|e| e.to_string())?;
                if request["nonce"] != control_nonce || request["after_reap"] != true {
                    return Err("invalid shutdown request".into());
                }
                common::publish(&control_dir.join("control-stop.json"),&json!({"nonce":control_nonce,"qpc":identity::qpc_ticks().map_err(|e|e.to_string())?,"shutdown_before":flag.load(Ordering::SeqCst),"request":request})).map_err(|e|e.to_string())?;
                flag.store(true, Ordering::SeqCst);
                return Ok(());
            }
        }
    });
    // run owns the real bind. Reservation release/bind gap is not called safe:
    // parent requires successful production listening log AND OS owning-PID proof.
    drop(reservation);
    let result = host::run(
        RemoteArgs {
            listen: addr,
            tls_cert: args[0].clone().into(),
            tls_key: args[1].clone().into(),
            token_file: args[2].clone().into(),
            log_file: None,
            mock_backend: true,
            feedback: FeedbackConfig::default(),
        },
        shutdown.clone(),
    );
    let _ = done_tx.send(());
    let control_result = control.join().map_err(|_| "control thread panicked")?;
    watchdog.finish();
    common::publish(
        &dir.join("supervisor-terminal.json"),
        &json!({"nonce":nonce,"run_ok":result.is_ok(),"shutdown":shutdown.load(Ordering::SeqCst),"control_joined":true,"control_ok":control_result.is_ok(),"watchdog_joined":true,"qpc":identity::qpc_ticks()?}),
    )?;
    result.map_err(|e| std::io::Error::other(e.to_string()))?;
    control_result.map_err(std::io::Error::other)?;
    if !shutdown.load(Ordering::SeqCst) {
        return Err("supervisor returned before explicit post-reap control".into());
    }
    Ok(())
}
