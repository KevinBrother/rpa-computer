//! Optional Host feedback adapter. No dependency is introduced in native-input.
//! Off never resolves a renderer, starts a thread, or opens a pipe.
mod authority;
mod backend;
mod config;
mod host;
mod pipe;
mod process;
pub(crate) mod worker;

pub use authority::{FeedbackHandle, RuntimePhase};
pub use backend::FactBackend;
pub use config::{FeedbackConfig, FeedbackError};
pub use host::{FeedbackHost, FeedbackShutdown};

#[cfg(test)]
mod tests;

#[cfg(all(test, windows))]
mod tests_process;
#[cfg(test)]
mod tests_runtime;

#[cfg(test)]
mod display_tests;
