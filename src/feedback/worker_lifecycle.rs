//! Worker construction and bounded teardown. Compiled as a worker child
//! module so private coordination state does not become a public API.
use super::*;

impl Worker {
    pub(super) fn start_config(
        factory: BackendFactory,
        config: crate::feedback::FeedbackConfig,
        shutdown: Option<Arc<AtomicBool>>,
    ) -> Result<Self, WorkerError> {
        Self::start_config_with_init_budget(factory, config, shutdown, INITIALIZATION_TIMEOUT)
    }

    pub(super) fn start_config_with_init_budget(
        factory: BackendFactory,
        config: crate::feedback::FeedbackConfig,
        shutdown: Option<Arc<AtomicBool>>,
        init_budget: Duration,
    ) -> Result<Self, WorkerError> {
        let cancel = Arc::new(AtomicBool::new(false));
        let generation = RequestGeneration::new();
        let (tx, rx) = sync_channel::<Command>(MAX_QUEUED_COMMANDS);
        let (ctrl_tx, ctrl_rx) = channel::<Control>();
        // No adapter allocation, child, pipe, path lookup or thread in off mode.
        let stop_cancel = cancel.clone();
        let stop_generation = generation.clone();
        let stop_control = ctrl_tx.clone();
        let (feedback, feedback_host) = if config.enabled() {
            crate::feedback::worker::start(
                &config,
                Arc::new(move || {
                    stop_generation.bump();
                    stop_cancel.store(true, Ordering::SeqCst);
                    if let Some(flag) = &shutdown {
                        flag.store(true, Ordering::SeqCst);
                    }
                    let _ = stop_control.send(Control::Quit);
                }),
            )
            .map_err(crate::feedback::worker::start_error)?
        } else {
            (None, None)
        };
        let native_feedback = feedback.clone();
        let (init_tx, init_rx) = channel::<Result<(), WorkerError>>();
        let session_started = Arc::new(Mutex::new(None::<Instant>));
        let shutdown_status = Arc::new(Mutex::new(None::<ShutdownStatus>));
        let stash_len = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let cancel_for_thread = Arc::clone(&cancel);
        let generation_for_thread = generation.clone();
        let started_for_thread = Arc::clone(&session_started);
        let status_for_thread = Arc::clone(&shutdown_status);
        let stash_for_thread = Arc::clone(&stash_len);

        let init_deadline = Instant::now() + init_budget;
        let join = std::thread::Builder::new()
            .name("computer-runtime".into())
            .spawn(move || {
                coordinator_main(
                    factory,
                    rx,
                    ctrl_rx,
                    init_tx,
                    CoordShared {
                        init_deadline,
                        cancel: cancel_for_thread,
                        generation: generation_for_thread,
                        session_started: started_for_thread,
                        shutdown_status: status_for_thread,
                        stash_len: stash_for_thread,
                        feedback: native_feedback,
                    },
                )
            })
            .map_err(|_| WorkerError::Dead)?;

        // Independent outer bound: even a coordinator failure cannot strand
        // Worker::start. The small confirmation grace lets the coordinator
        // publish its native deadline classification, not extend native setup.
        let outer_wait =
            init_deadline.saturating_duration_since(Instant::now()) + WATCHDOG_POLL * 2;
        match init_rx.recv_timeout(outer_wait) {
            Ok(Ok(())) if Instant::now() < init_deadline && !cancel.load(Ordering::SeqCst) => {
                let transport_gate = TransportGate::new();
                let active = transport_gate.active_view();
                Ok(Worker {
                    handle: Some(WorkerHandle {
                        tx,
                        ctrl_tx,
                        session_started,
                        join: Mutex::new(Some(join)),
                        shutdown_status,
                        stash_len,
                    }),
                    cancel,
                    active,
                    generation,
                    transport_gate,
                    faulted: Arc::new(AtomicBool::new(false)),
                    feedback,
                    feedback_host: Mutex::new(feedback_host),
                })
            }
            result => {
                generation.bump();
                cancel.store(true, Ordering::SeqCst);
                let _ = ctrl_tx.send(Control::Quit);
                // Close both ends exposed to the starter before returning.
                // There is no Worker handle through which late work can enter.
                drop(tx);
                drop(init_rx);
                if join.is_finished() && join.join().is_err() {
                    return Err(native::init_uncertain(
                        "initialization_coordinator_panicked",
                        "coordinator panicked during startup",
                    ));
                }
                match result {
                    Ok(Err(e)) => Err(e),
                    _ => Err(native::init_uncertain(
                        "initialization_unconfirmed",
                        "readiness was not confirmed within the startup budget",
                    )),
                }
            }
        }
    }

    pub(super) fn shutdown_native_worker(&self) -> ShutdownStatus {
        // Epoch first, then the flag (see CancelHandle::cancel).
        self.generation.bump();
        self.cancel.store(true, Ordering::SeqCst);
        self.active.set(None);
        let Some(handle) = self.handle.as_ref() else {
            return ShutdownStatus::NotNeeded;
        };
        let join = handle.join.lock().ok().and_then(|mut g| g.take());
        let Some(join) = join else {
            // Already shut down once; report the recorded outcome.
            return handle
                .shutdown_status
                .lock()
                .ok()
                .and_then(|g| *g)
                .unwrap_or(ShutdownStatus::NotNeeded);
        };
        // Quit is control, not data: the control channel is unbounded, so a
        // flooded client queue can never delay shutdown.
        let _ = handle.ctrl_tx.send(Control::Quit);
        let (done_tx, done_rx) = channel();
        let spawned = std::thread::Builder::new()
            .name("computer-worker-join".into())
            .spawn(move || {
                let _ = join.join();
                let _ = done_tx.send(());
            });
        if spawned.is_err() {
            // Could not even spawn the join helper: a bounded join is
            // impossible; report unknown honestly.
            eprintln!("[computer-host] cannot spawn join helper; shutdown_unknown");
            self.mark_unknown(handle);
            return ShutdownStatus::Unknown;
        }
        match done_rx.recv_timeout(MAX_SHUTDOWN_CONFIRM) {
            Ok(()) => {
                let status = handle
                    .shutdown_status
                    .lock()
                    .ok()
                    .and_then(|g| *g)
                    .unwrap_or(ShutdownStatus::Clean);
                // A faulted worker (abandoned native call, failed cleanup)
                // is NEVER reported clean — even if the coordinator managed
                // to exit. Unknown sticks.
                if shutdown_is_quarantined(status) || self.is_faulted() {
                    self.mark_unknown(handle);
                    return ShutdownStatus::Unknown;
                }
                status
            }
            Err(_) => {
                eprintln!(
                    "[computer-host] shutdown confirmation exceeded {}s; reporting shutdown_unknown \
                     (a native call never returned; cleanup state cannot be guaranteed)",
                    MAX_SHUTDOWN_CONFIRM.as_secs()
                );
                self.mark_unknown(handle);
                ShutdownStatus::Unknown
            }
        }
    }
}
