//! Explicit backend construction for the worker thread.
//!
//! Production always uses [`BackendFactory::Desktop`]. `Mock` is the
//! explicit `--mock-backend` diagnostic mode (in-memory fake desktop; no
//! real lock/hotkey/input) parsed in `computer-host.rs`. The `Test` variant
//! exists only so transport tests can inject a fake backend. Neither `Mock`
//! nor `Test` is ever a silent fallback from native errors.

use crate::backend::{Backend, BackendError};

pub enum BackendFactory {
    /// Real native desktop backend (default and only production mode).
    Desktop,
    /// Explicit `--mock-backend` diagnostic mode: in-memory fake desktop,
    /// no real lock/hotkey/input. Activated ONLY by the CLI flag parsed in
    /// `computer-host.rs`; never a fallback from native errors.
    Mock,
    /// Test-only injected backend. Never activated implicitly.
    Test(Box<dyn FnOnce() -> Result<Box<dyn Backend>, BackendError> + Send>),
}

impl std::fmt::Debug for BackendFactory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackendFactory::Desktop => write!(f, "BackendFactory::Desktop"),
            BackendFactory::Mock => write!(f, "BackendFactory::Mock"),
            BackendFactory::Test(_) => write!(f, "BackendFactory::Test(..)"),
        }
    }
}
