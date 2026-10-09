use rpa_desktop_feedback::{process::RendererCommand, Diagnostic};
use std::path::PathBuf;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FeedbackConfig {
    pub executable: Option<PathBuf>,
    pub accent: Option<String>,
    pub label: Option<String>,
}
impl FeedbackConfig {
    pub fn enabled(&self) -> bool {
        self.executable.is_some()
    }
    pub fn validate(&self) -> Result<(), FeedbackError> {
        if !self.enabled() {
            return if self.accent.is_some() || self.label.is_some() {
                Err(FeedbackError::StyleWithoutRenderer)
            } else {
                Ok(())
            };
        }
        self.command().map(|_| ())
    }
    pub(crate) fn command(&self) -> Result<RendererCommand, FeedbackError> {
        RendererCommand::new(
            self.executable
                .clone()
                .ok_or(FeedbackError::StyleWithoutRenderer)?,
            self.accent.clone(),
            self.label.clone(),
        )
        .map_err(FeedbackError::Protocol)
    }
    /// Append only feedback style/path. Never forward TLS keys, tokens or logs.
    pub fn append_args(&self, command: &mut std::process::Command) {
        if let Some(path) = &self.executable {
            command.arg("--desktop-feedback").arg(path);
            if let Some(accent) = &self.accent {
                command.arg("--feedback-accent").arg(accent);
            }
            if let Some(label) = &self.label {
                command.arg("--feedback-label").arg(label);
            }
        }
    }
    pub fn description(&self) -> serde_json::Value {
        serde_json::json!({"enabled": self.enabled(), "capture_exclusion": if self.enabled() { "unverified_until_ready" } else { "not_applicable" }, "requested_is_verified": false, "renderer_loss_policy": "terminate_host_child_control"})
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedbackError {
    StyleWithoutRenderer,
    Protocol(Diagnostic),
    CaptureUnsupported,
    WorkerUnavailable,
    AuthorityRevoked,
    AuthorityUnavailable,
}
impl std::fmt::Display for FeedbackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StyleWithoutRenderer => f.write_str("desktop_feedback: --feedback-accent/label require --desktop-feedback"),
            Self::CaptureUnsupported => f.write_str("desktop_feedback: capture_exclusion_unsupported (the current capture backend cannot exclude this renderer; requested is not verification)"),
            Self::Protocol(d) => d.fmt(f),
            other => write!(f, "desktop_feedback:{other:?}"),
        }
    }
}
impl std::error::Error for FeedbackError {}
