use std::fmt;

/// No application name, bundle identifier, window title or localized NSError text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    Unsupported,
    PermissionDenied,
    NoDisplay,
    ExcludedProcessMissing,
    InvalidRequest,
    Busy,
    Timeout,
    CaptureFailed,
    EncodingFailed,
    InvalidImage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Availability,
    Preflight,
    Validate,
    Discovery,
    Capture,
    Encode,
    Wait,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeErrorDomain {
    ScreenCaptureKit,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureError {
    pub kind: ErrorKind,
    pub stage: Stage,
    pub native_domain: Option<NativeErrorDomain>,
    pub native_code: Option<i64>,
}

impl CaptureError {
    pub const fn new(kind: ErrorKind, stage: Stage) -> Self {
        Self {
            kind,
            stage,
            native_domain: None,
            native_code: None,
        }
    }

    pub const fn with_native(
        kind: ErrorKind,
        stage: Stage,
        domain: NativeErrorDomain,
        code: i64,
    ) -> Self {
        Self {
            kind,
            stage,
            native_domain: Some(domain),
            native_code: Some(code),
        }
    }

    pub const fn code(&self) -> &'static str {
        match self.kind {
            ErrorKind::Unsupported => "capture_unsupported",
            ErrorKind::PermissionDenied => "capture_permission_denied",
            ErrorKind::NoDisplay => "capture_no_display",
            ErrorKind::ExcludedProcessMissing => "capture_excluded_process_missing",
            ErrorKind::InvalidRequest => "capture_invalid_request",
            ErrorKind::Busy => "capture_busy",
            ErrorKind::Timeout => "capture_timeout",
            ErrorKind::CaptureFailed => "capture_failed",
            ErrorKind::EncodingFailed => "capture_encoding_failed",
            ErrorKind::InvalidImage => "capture_invalid_image",
        }
    }
}

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({:?})", self.code(), self.stage)?;
        if let Some(code) = self.native_code {
            write!(f, " native_code={code}")?;
        }
        Ok(())
    }
}
impl std::error::Error for CaptureError {}
pub type Result<T> = std::result::Result<T, CaptureError>;

/// Values verified against Apple's SDK SCError.h / official objc2 SCError.rs.
/// Domain must match as well: an arbitrary NSError -3801 is not permission denial.
pub(crate) const fn classify_native_error(domain: NativeErrorDomain, code: i64) -> ErrorKind {
    match (domain, code) {
        (NativeErrorDomain::ScreenCaptureKit, -3801 | -3803) => ErrorKind::PermissionDenied,
        (NativeErrorDomain::ScreenCaptureKit, -3814 | -3815) => ErrorKind::NoDisplay,
        _ => ErrorKind::CaptureFailed,
    }
}
