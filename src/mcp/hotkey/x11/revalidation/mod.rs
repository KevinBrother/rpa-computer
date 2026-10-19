//! Full, coherent semantic observation on the dedicated XCB connection.
//! An equivalent proof never changes grabs/readiness or grants input by itself.
mod capture;
#[cfg(test)]
mod comparison_diagnostic;
#[cfg(test)]
mod fixture;
mod initial_handoff;
#[cfg(test)]
mod initial_handoff_tests;
#[cfg(test)]
mod integration_tests;
mod live;
mod queue;
mod scratch;
pub(super) use initial_handoff::finish_initial;
#[cfg(test)]
mod tests;
use super::super::x11_plan::HotkeyPlan;
use super::semantic_image::SemanticImage;
use super::xkb_map::Validated;
pub(super) use live::LiveSource;
use std::collections::VecDeque;
use std::time::Instant;

pub(super) struct Baseline {
    pub generation: usize,
    pub image: SemanticImage,
    pub plan: HotkeyPlan,
    pub validated: Validated,
    // Init-only aggregate count: all capture + post-grab queued poll attempts,
    // including None. No reset at the grab/readiness boundary.
    initial_queue_polls: usize,
}
impl Baseline {
    pub fn validated(generation: usize, image: SemanticImage) -> Result<Self, String> {
        let (plan, validated) = image.validate()?;
        Ok(Self {
            generation,
            image,
            plan,
            validated,
            initial_queue_polls: 0,
        })
    }
}
#[derive(Clone, Copy)]
pub(super) struct Notice {
    pub device: u8,
    pub old_device: u8,
    pub min: u8,
    pub max: u8,
    pub old_min: u8,
    pub old_max: u8,
    pub changed: u16,
    pub synthetic: bool,
}
impl Notice {
    pub fn from_event(e: &xcb::xkb::NewKeyboardNotifyEvent) -> Self {
        Self {
            device: e.device_id(),
            old_device: e.old_device_id(),
            min: e.min_key_code(),
            max: e.max_key_code(),
            old_min: e.old_min_key_code(),
            old_max: e.old_max_key_code(),
            changed: e.changed().bits() as u16,
            synthetic: e.response_type() & 0x80 != 0,
        }
    }
}
#[derive(Clone, Copy)]
pub(super) enum ScratchPart {
    Symbols,
    Types,
    Actions,
    Behaviors,
    Explicit,
}
#[derive(Clone, Copy)]
pub(super) struct Scratch {
    pub part: ScratchPart,
    pub first: u8,
    pub count: u8,
}
#[derive(Clone, Copy)]
pub(super) enum Trigger {
    Equivalent(Notice),
    Scratch(Scratch),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Invalidation {
    NewKeyboard,
    CoreMapping,
    Map,
    Controls,
    Compatibility,
}

/// Adapter supplies owned observations and actual events/errors, not an
/// `equivalent`/`safe`/`stable` flag. Every suboperation receives ONE deadline.
pub(super) trait Source {
    type Event;
    fn generation(&self) -> usize;
    fn checkpoint(&mut self, deadline: Instant) -> Result<(), String>;
    fn read_image(&mut self, deadline: Instant) -> Result<SemanticImage, String>;
    fn fence(&mut self, deadline: Instant) -> Result<(), String>;
    fn queued(&mut self, deadline: Instant) -> Result<Option<Self::Event>, String>;
    fn invalidation(&self, event: &Self::Event) -> Option<Invalidation>;
}

pub(super) struct EquivalentProof {
    generation: usize,
}
impl EquivalentProof {
    pub fn generation(&self) -> usize {
        self.generation
    }
}

pub(super) fn capture_initial<S: Source>(
    source: &mut S,
    pending: &mut VecDeque<S::Event>,
    deadline: Instant,
) -> Result<Baseline, String> {
    capture::stable(source, pending, deadline)
}

/// Known same-device/range NKN metadata only admits an ATTEMPT. It cannot prove
/// equivalence, provenance or absence of subsequent invalidation by itself.
fn check_notice(baseline: &Baseline, n: Notice) -> Result<(), String> {
    let image = &baseline.image.map;
    if n.synthetic
        || n.changed == 0
        || n.changed & !3 != 0
        || n.device != image.device
        || n.old_device != image.device
        || n.min != image.min_key
        || n.old_min != image.min_key
        || n.max != image.max_key
        || n.old_max != image.max_key
    {
        return Err("XKB NKN is not an eligible same-identity observation trigger".into());
    }
    Ok(())
}

/// Fixed A/B/fence, zero retries, one absolute deadline and shared queue bound.
/// No writes to active/ready/cancel, no ungrab/regrab and no failure resurrection.
pub(super) fn revalidate<S: Source>(
    baseline: &mut Baseline,
    trigger: Trigger,
    source: &mut S,
    pending: &mut VecDeque<S::Event>,
    deadline: Instant,
) -> Result<EquivalentProof, String> {
    let generation = baseline.generation;
    queue::checkpoint(source, generation, deadline)?;
    match trigger {
        Trigger::Equivalent(notice) => check_notice(baseline, notice)?,
        Trigger::Scratch(change) => {
            scratch::range(baseline, change)?;
        }
    }
    let mut observed = capture::stable(source, pending, deadline)?;
    if observed.generation != generation {
        return Err("XKB captured a different registration generation".into());
    }
    match trigger {
        Trigger::Equivalent(_) => {
            #[cfg(test)]
            comparison_diagnostic::first(baseline, &observed);
            if observed.image != baseline.image
                || observed.plan != baseline.plan
                || observed.validated.modifiers != baseline.validated.modifiers
            {
                return Err("XKB full NKN configuration differs from authorized baseline".into());
            }
        }
        Trigger::Scratch(change) => scratch::validate_delta(baseline, &observed, change)?,
    }
    queue::checkpoint(source, generation, deadline)?;
    // Worker-local cache assignment, NOT readiness/activation publication. It
    // does not touch lifecycle state or emit input callbacks. Keep the original
    // init accounting; this capture's new operation count must not reset it.
    observed.initial_queue_polls = baseline.initial_queue_polls;
    let previous = std::mem::replace(baseline, observed);
    if let Err(error) = queue::checkpoint(source, generation, deadline) {
        *baseline = previous;
        return Err(error);
    }
    Ok(EquivalentProof { generation })
}

pub(super) fn scratch_notice(event: &xcb::Event) -> Result<Option<Scratch>, String> {
    scratch::notice(event)
}
