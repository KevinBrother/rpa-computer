//! Worker bridge kept out of the already-large coordinator module.
use super::{FeedbackHandle, FeedbackHost, FeedbackShutdown};
use crate::mcp::worker::ShutdownStatus;
use rpa_desktop_feedback::protocol::Cleanup;
use std::sync::Mutex;

pub(crate) fn record_shutdown(facts: Option<&FeedbackHandle>, status: ShutdownStatus) {
    if let Some(f) = facts {
        // Native Runtime publishes the precise successful result. The worker
        // adds ONLY quarantine/unknown: an abandoned call's late release must
        // never overwrite this. Clean is not invented from process exit.
        if crate::mcp::worker::shutdown_is_quarantined(status) {
            f.terminate();
            f.complete_terminal(Cleanup::Unknown);
            eprintln!(
                "[computer-host] desktop_feedback native cleanup unknown; worker quarantined"
            );
        }
    }
}
pub(crate) fn stop_renderer(host: &Mutex<Option<FeedbackHost>>) -> bool {
    let Ok(mut slot) = host.lock() else {
        return false;
    };
    let Some(mut host) = slot.take() else {
        return true;
    };
    let status = host.shutdown();
    if status != FeedbackShutdown::Reaped {
        eprintln!(
            "[computer-host] desktop_feedback teardown {status:?}; refusing safe shutdown claim"
        );
    }
    status == FeedbackShutdown::Reaped
}

pub(crate) fn start(
    config: &super::FeedbackConfig,
    on_stop: std::sync::Arc<dyn Fn() + Send + Sync>,
) -> Result<(Option<FeedbackHandle>, Option<FeedbackHost>), super::FeedbackError> {
    if !config.enabled() {
        return Ok((None, None));
    }
    let facts = FeedbackHandle::new(on_stop)?;
    let host = FeedbackHost::start(config, facts.clone())?;
    Ok((Some(facts), host))
}
pub(crate) fn start_error(error: super::FeedbackError) -> crate::mcp::worker::WorkerError {
    crate::mcp::worker::WorkerError::BackendUnavailable {
        code: "desktop_feedback_startup_rejected".into(),
        message: error.to_string(),
    }
}
