//! Exact v1 wire types. Unknown and duplicate fields are rejected by serde.
//! Use the codec or `validate()` at every public boundary; raw struct construction
//! does not itself imply validity. No extensible data/metadata payload is allowed.
use crate::Diagnostic;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct V1;
impl Serialize for V1 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(1)
    }
}
impl<'de> Deserialize<'de> for V1 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match u64::deserialize(deserializer)? {
            1 => Ok(Self),
            _ => Err(serde::de::Error::custom(
                "unsupported desktop feedback version",
            )),
        }
    }
}

fn required_nullable<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::deserialize(deserializer)
}

/// Identifiers are opaque bounded ASCII tokens, not user labels or app titles.
/// Generation zero is valid on the wire; Host grant policy must prevent reuse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Session {
    pub id: String,
    pub generation: u64,
}
impl Session {
    pub fn validate(&self) -> Result<(), Diagnostic> {
        validate_token(&self.id, 128)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Starting,
    Idle,
    Observing,
    Executing,
    Paused,
    Stopping,
    Faulted,
    Closed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cleanup {
    NotNeeded,
    Pending,
    Released,
    Failed,
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PointerKind {
    Move,
    Click,
    Drag,
}

/// Native input coordinates, never screenshot pixels. Negative origins allowed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Surface {
    pub id: String,
    /// Bounded opaque Host geometry version, including origin comma (e.g.
    /// `d1:o0,0:i1512x982:c3024x1964:r0`), not a session identity token.
    pub version: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
impl Surface {
    pub fn validate(&self) -> Result<(), Diagnostic> {
        validate_token(&self.id, 128)?;
        validate_surface_version(&self.version)?;
        if [self.x, self.y, self.width, self.height]
            .iter()
            .any(|v| !v.is_finite())
            || self.width <= 0.
            || self.height <= 0.
            || !(self.x + self.width).is_finite()
            || !(self.y + self.height).is_finite()
        {
            return Err(Diagnostic::InvalidField);
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pointer {
    pub x: f64,
    pub y: f64,
    pub kind: PointerKind,
}

/// Complete authoritative snapshot, not a delta or an animation event queue.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub version: V1,
    pub sequence: u64,
    #[serde(deserialize_with = "required_nullable")]
    pub session: Option<Session>,
    pub phase: Phase,
    pub cleanup: Cleanup,
    #[serde(deserialize_with = "required_nullable")]
    pub surface: Option<Surface>,
    #[serde(deserialize_with = "required_nullable")]
    pub pointer: Option<Pointer>,
}
impl Snapshot {
    pub fn validate(&self) -> Result<(), Diagnostic> {
        if let Some(session) = &self.session {
            session.validate()?;
        }
        if let Some(surface) = &self.surface {
            surface.validate()?;
        }
        if let Some(pointer) = &self.pointer {
            if self.surface.is_none() || !pointer.x.is_finite() || !pointer.y.is_finite() {
                return Err(Diagnostic::InvalidField);
            }
        }
        Ok(())
    }
    /// Presentation helper only: transport success never implies cleanup success.
    pub fn safe_completion(&self) -> bool {
        self.phase == Phase::Closed
            && matches!(self.cleanup, Cleanup::Released | Cleanup::NotNeeded)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum HostMessage {
    Snapshot(Snapshot),
}
impl HostMessage {
    pub fn validate(&self) -> Result<(), Diagnostic> {
        match self {
            Self::Snapshot(s) => s.validate(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureExclusion {
    Requested,
    Unsupported,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ready {
    pub version: V1,
    pub capture_exclusion: CaptureExclusion,
    pub pointer_feedback: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stop {
    pub version: V1,
    pub session: Session,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Heartbeat {
    pub version: V1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RendererError {
    pub version: V1,
    pub code: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum RendererMessage {
    Ready(Ready),
    Stop(Stop),
    Heartbeat(Heartbeat),
    Error(RendererError),
}
impl RendererMessage {
    pub fn validate(&self) -> Result<(), Diagnostic> {
        match self {
            Self::Stop(s) => s.session.validate(),
            Self::Error(e) => validate_token(&e.code, 64),
            Self::Ready(_) | Self::Heartbeat(_) => Ok(()),
        }
    }
}

pub(crate) fn validate_token(value: &str, max: usize) -> Result<(), Diagnostic> {
    validate_ascii_token(value, max, b"-_.:")
}

fn validate_surface_version(value: &str) -> Result<(), Diagnostic> {
    // Host Geometry.version has a comma separating signed display origins.
    // Keep this separate so renderer compatibility never loosens session IDs.
    validate_ascii_token(value, 128, b"-_.:,")
}

fn validate_ascii_token(value: &str, max: usize, punctuation: &[u8]) -> Result<(), Diagnostic> {
    if value.is_empty()
        || value.len() > max
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || punctuation.contains(&b))
    {
        return Err(Diagnostic::InvalidField);
    }
    Ok(())
}
