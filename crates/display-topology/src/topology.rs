use crate::{Error, NativeRect, PixelSize};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TRACKER: AtomicU64 = AtomicU64::new(1);

/// Explicit coordinate unit shared by all descriptors in a snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeUnit {
    /// Native logical points, e.g. macOS global input coordinates.
    Points,
    /// Native physical pixels; Windows caller must establish PMv2 awareness.
    PhysicalPixels,
}

/// Clockwise rotation reported by enumeration. Sources MUST already be upright
/// in the native global axes; the geometry library does not rotate raw pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rotation {
    /// Zero degrees.
    Deg0,
    /// Ninety degrees.
    Deg90,
    /// One hundred eighty degrees.
    Deg180,
    /// Two hundred seventy degrees.
    Deg270,
}

/// Complete runtime display facts supplied by a real enumeration backend.
/// IDs are opaque and are NOT promised stable across unplug/replug.
#[derive(Debug, Clone, PartialEq)]
pub struct DisplayDescriptor {
    /// Runtime opaque ID, never interpreted or normalized by this library.
    pub id: String,
    /// Exactly one enumerated display must be primary.
    pub is_primary: bool,
    /// Bounds in the explicitly declared native unit, independent of capture.
    pub native_bounds: NativeRect,
    /// Actual, already oriented source capture dimensions.
    pub capture_size: PixelSize,
    /// Finite positive enumeration scale fact; not used to guess source size.
    pub scale: f64,
    /// Enumerated rotation fact, included in exact layout comparison.
    pub rotation: Rotation,
}
impl DisplayDescriptor {
    fn validate(&self) -> Result<(), Error> {
        if self.id.is_empty() {
            return Err(Error::EmptyDisplayId);
        }
        self.native_bounds.validate(&self.id)?;
        if !self.capture_size.valid() {
            return Err(Error::InvalidCaptureSize {
                id: self.id.clone(),
            });
        }
        self.capture_size.area()?;
        if !self.scale.is_finite() || self.scale <= 0.0 {
            return Err(Error::InvalidScale {
                id: self.id.clone(),
            });
        }
        Ok(())
    }
    fn exact_eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.is_primary == other.is_primary
            && self.native_bounds == other.native_bounds
            && self.capture_size == other.capture_size
            && self.scale.to_bits() == other.scale.to_bits()
            && self.rotation == other.rotation
    }
}

/// No fallback policy: an explicit missing display is an error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    /// Select the sole currently enumerated primary display.
    Primary,
    /// Select this exact opaque runtime ID.
    Display(String),
    /// Select every enumerated display with preserved spatial layout.
    Desktop,
}

/// Process-local tracker identity plus a checked monotonic revision.
/// Never persist this token across Host restarts; refresh observation instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Generation {
    tracker: u64,
    revision: u64,
}
impl Generation {
    /// Encode the complete process-local authority and revision as an opaque token.
    ///
    /// Exactly 59 ASCII bytes: `topology:tracker:` followed by 16 lowercase hex
    /// digits, then `:revision:` and 16 lowercase hex digits. Both u64 fields
    /// are encoded losslessly; distinct generations never share a token within
    /// this process. No Debug formatting, hashing, or truncation is involved.
    /// The token uses only ASCII letters, digits and colons and fits bounded
    /// 128-byte Host geometry-version fields. It is not persistent identity:
    /// tracker allocation restarts with the process. Refresh after Host restart.
    pub fn to_token(self) -> String {
        format!(
            "topology:tracker:{:016x}:revision:{:016x}",
            self.tracker, self.revision
        )
    }

    /// Revision in this tracker lifetime, starting at one after first update.
    pub fn revision(self) -> u64 {
        self.revision
    }
    pub(crate) fn require(self, current: Self) -> Result<(), Error> {
        if self == current {
            Ok(())
        } else {
            Err(Error::StaleGeneration {
                expected: self,
                actual: current,
            })
        }
    }
}

/// Immutable validated, ID-sorted complete topology facts and generation.
#[derive(Debug, Clone, PartialEq)]
pub struct TopologySnapshot {
    generation: Generation,
    unit: NativeUnit,
    displays: Vec<DisplayDescriptor>,
}
impl TopologySnapshot {
    /// Token that must match at action preflight and each drag interpolation.
    pub fn generation(&self) -> Generation {
        self.generation
    }
    /// Unit of every native coordinate in this snapshot.
    pub fn unit(&self) -> NativeUnit {
        self.unit
    }
    /// Complete descriptors sorted by exact, case-sensitive opaque ID.
    pub fn displays(&self) -> &[DisplayDescriptor] {
        &self.displays
    }
    pub(crate) fn select(&self, selection: &Selection) -> Result<Vec<&DisplayDescriptor>, Error> {
        match selection {
            Selection::Desktop => Ok(self.displays.iter().collect()),
            Selection::Primary => Ok(self.displays.iter().filter(|d| d.is_primary).collect()),
            Selection::Display(id) => self
                .displays
                .iter()
                .find(|d| &d.id == id)
                .map(|d| vec![d])
                .ok_or_else(|| Error::DisplayNotFound { id: id.clone() }),
        }
    }
}

/// One topology authority for a Host/session lifetime. Successful updates compare
/// ALL sorted facts exactly, not hashes. Invalid updates/exhaustion are atomic.
#[derive(Debug)]
pub struct TopologyTracker {
    identity: u64,
    latest: Option<TopologySnapshot>,
}
impl TopologyTracker {
    /// Create an independent process-local lifetime, never aliasing old revisions.
    pub fn new() -> Result<Self, Error> {
        let identity = NEXT_TRACKER
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| Error::TrackerIdentityExhausted)?;
        Ok(Self {
            identity,
            latest: None,
        })
    }
    /// Validate real enumeration and return a snapshot. Order-only changes keep
    /// generation; unit/layout/DPI/identity/primary/capture/rotation changes do not.
    pub fn update(
        &mut self,
        unit: NativeUnit,
        mut displays: Vec<DisplayDescriptor>,
    ) -> Result<TopologySnapshot, Error> {
        if displays.is_empty() {
            return Err(Error::EmptyTopology);
        }
        displays.sort_by(|a, b| a.id.cmp(&b.id));
        for display in &displays {
            display.validate()?;
        }
        for pair in displays.windows(2) {
            if pair[0].id == pair[1].id {
                return Err(Error::DuplicateDisplayId {
                    id: pair[0].id.clone(),
                });
            }
        }
        let primaries: Vec<_> = displays
            .iter()
            .filter(|d| d.is_primary)
            .map(|d| d.id.clone())
            .collect();
        match primaries.len() {
            0 => return Err(Error::NoPrimaryDisplay),
            1 => {}
            _ => return Err(Error::MultiplePrimaryDisplays { ids: primaries }),
        }
        let revision = if let Some(old) = &self.latest {
            if old.unit == unit
                && old.displays.len() == displays.len()
                && old
                    .displays
                    .iter()
                    .zip(&displays)
                    .all(|(a, b)| a.exact_eq(b))
            {
                return Ok(old.clone());
            }
            old.generation
                .revision
                .checked_add(1)
                .ok_or(Error::GenerationExhausted)?
        } else {
            1
        };
        let snapshot = TopologySnapshot {
            generation: Generation {
                tracker: self.identity,
                revision,
            },
            unit,
            displays,
        };
        self.latest = Some(snapshot.clone());
        Ok(snapshot)
    }
    /// Last successfully committed snapshot; enumeration errors leave it unchanged.
    /// This is NOT evidence that the physical desktop is still current.
    pub fn latest(&self) -> Option<&TopologySnapshot> {
        self.latest.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generation_exhaustion_is_checked_without_mutating_facts() {
        let d = DisplayDescriptor {
            id: "one".into(),
            is_primary: true,
            native_bounds: NativeRect {
                x: 0,
                y: 0,
                width: 10,
                height: 10,
            },
            capture_size: PixelSize {
                width: 10,
                height: 10,
            },
            scale: 1.0,
            rotation: Rotation::Deg0,
        };
        let mut tracker = TopologyTracker::new().unwrap();
        tracker.update(NativeUnit::Points, vec![d.clone()]).unwrap();
        tracker.latest.as_mut().unwrap().generation.revision = u64::MAX;
        let original = tracker.latest().unwrap().clone();
        let mut changed = d.clone();
        changed.scale = 2.0;
        assert_eq!(
            tracker
                .update(NativeUnit::Points, vec![changed])
                .unwrap_err(),
            Error::GenerationExhausted
        );
        assert_eq!(tracker.latest(), Some(&original));
        assert_eq!(
            tracker
                .update(NativeUnit::Points, vec![d])
                .unwrap()
                .generation()
                .revision(),
            u64::MAX
        );
    }
}

#[cfg(test)]
mod token_tests {
    use super::*;

    fn display() -> DisplayDescriptor {
        DisplayDescriptor {
            id: "one".into(),
            is_primary: true,
            native_bounds: NativeRect {
                x: -10,
                y: 20,
                width: 100,
                height: 80,
            },
            capture_size: PixelSize {
                width: 100,
                height: 80,
            },
            scale: 1.0,
            rotation: Rotation::Deg0,
        }
    }

    #[test]
    fn token_is_stable_for_unchanged_full_generation() {
        let mut tracker = TopologyTracker::new().unwrap();
        let first = tracker
            .update(NativeUnit::PhysicalPixels, vec![display()])
            .unwrap();
        let repeated = tracker
            .update(NativeUnit::PhysicalPixels, vec![display()])
            .unwrap();
        assert_eq!(first.generation(), repeated.generation());
        assert_eq!(
            first.generation().to_token(),
            repeated.generation().to_token()
        );
        assert_eq!(first.generation().to_token(), first.generation().to_token());
    }

    #[test]
    fn token_distinguishes_independent_trackers_at_same_revision() {
        let first = TopologyTracker::new()
            .unwrap()
            .update(NativeUnit::PhysicalPixels, vec![display()])
            .unwrap()
            .generation();
        let second = TopologyTracker::new()
            .unwrap()
            .update(NativeUnit::PhysicalPixels, vec![display()])
            .unwrap()
            .generation();
        assert_eq!(first.revision(), second.revision());
        assert_ne!(first, second);
        assert_ne!(first.to_token(), second.to_token());
    }

    #[test]
    fn token_changes_when_revision_changes() {
        let mut tracker = TopologyTracker::new().unwrap();
        let first = tracker
            .update(NativeUnit::PhysicalPixels, vec![display()])
            .unwrap()
            .generation();
        let mut changed = display();
        changed.scale = 2.0;
        let next = tracker
            .update(NativeUnit::PhysicalPixels, vec![changed])
            .unwrap()
            .generation();
        assert_eq!(first.tracker, next.tracker);
        assert_eq!(next.revision(), first.revision() + 1);
        assert_ne!(first.to_token(), next.to_token());
    }

    #[test]
    fn token_is_bounded_ascii_at_u64_boundaries() {
        // Private construction exercises all bit patterns without consuming or
        // modifying the process-global authority allocator.
        for tracker in [0, 1, u64::MAX - 1, u64::MAX] {
            for revision in [0, 1, u64::MAX - 1, u64::MAX] {
                let token = Generation { tracker, revision }.to_token();
                assert_eq!(token.len(), 59);
                assert!(token.len() <= 128);
                assert!(token.is_ascii());
                assert!(token
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.:,".contains(&b)));
            }
        }
        assert_eq!(
            Generation {
                tracker: u64::MAX,
                revision: u64::MAX
            }
            .to_token(),
            "topology:tracker:ffffffffffffffff:revision:ffffffffffffffff"
        );
        assert_eq!(
            Generation {
                tracker: 0,
                revision: 0
            }
            .to_token(),
            "topology:tracker:0000000000000000:revision:0000000000000000"
        );
    }

    #[test]
    fn token_keeps_both_full_fields_without_concatenation_collisions() {
        let mut tokens = std::collections::HashSet::new();
        for tracker in [0, 1, 12, 23, u64::MAX - 1, u64::MAX] {
            for revision in [0, 1, 12, 23, u64::MAX - 1, u64::MAX] {
                let token = Generation { tracker, revision }.to_token();
                assert!(
                    tokens.insert(token.clone()),
                    "aliased pair ({tracker}, {revision})"
                );
                // Fixed-width exact fields, not a lossy hash or Debug filtering.
                let fields: Vec<_> = token.split(':').collect();
                assert_eq!(fields.len(), 5);
                assert_eq!(u64::from_str_radix(fields[2], 16).unwrap(), tracker);
                assert_eq!(u64::from_str_radix(fields[4], 16).unwrap(), revision);
            }
        }
        assert_eq!(tokens.len(), 36);
    }
}
