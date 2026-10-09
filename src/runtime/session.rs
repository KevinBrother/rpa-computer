//! Session state machine, observation store, request ledger, and step
//! execution with truthful outcome reporting.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::backend::{Backend, Geometry};
use crate::runtime::actions::{self, Action};
use crate::runtime::error::{codes, ToolError};
use crate::runtime::execute;
use crate::runtime::image::{self, CoordMap};

/// Maximum age of the observation a step may be based on.
pub const OBSERVATION_FRESHNESS: Duration = Duration::from_secs(120);
/// Overall deadline for one step call.
pub const STEP_DEADLINE: Duration = Duration::from_secs(15);
/// Maximum time the input-dispatch phase of a step may take.
pub const INPUT_PHASE_BUDGET: Duration = Duration::from_secs(5);
/// Maximum time the observation phase of a step may take.
pub const OBSERVE_PHASE_BUDGET: Duration = Duration::from_secs(3);
/// Maximum bounded wait inside `computer_observe`.
pub const OBSERVE_MAX_WAIT_MS: u64 = 3000;
/// Settle delay between input dispatch and the post-step observation.
pub const SETTLE_DELAY: Duration = Duration::from_millis(300);
/// Slice granularity for cancellation-checked sleeps.
pub const CANCEL_CHECK_SLICE: Duration = Duration::from_millis(25);
/// Maximum requests remembered per session; beyond this new steps are
/// refused. Every registered request keeps its full outcome metadata for the
/// entire session — only images are evicted — so a replay or get_step of any
/// of them can never become falsely "unknown".
pub const MAX_REQUESTS_PER_SESSION: usize = 1000;
/// Maximum number of step results whose observation PNG bytes are retained.
/// Older results keep all metadata; only their images are dropped.
pub const MAX_RESULT_IMAGES_PER_SESSION: usize = 8;
/// Maximum image bytes retained per session across the image cache (current,
/// previous, and result observations). All retained PNGs are accounted in
/// exactly one place.
pub const MAX_IMAGE_BYTES_PER_SESSION: u64 = 32 * 1024 * 1024;
/// Default image bound for `computer_open`.
pub const DEFAULT_MAX_WIDTH: u32 = 1366;
pub const DEFAULT_MAX_HEIGHT: u32 = 768;
pub const MAX_OPEN_DIMENSION: u32 = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Ready,
    Paused,
    Faulted,
    Closed,
}

impl SessionState {
    pub fn name(self) -> &'static str {
        match self {
            SessionState::Ready => "ready",
            SessionState::Paused => "paused",
            SessionState::Faulted => "faulted",
            SessionState::Closed => "closed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputOutcome {
    NotStarted,
    Dispatched,
    Partial,
    Unknown,
}

impl InputOutcome {
    pub fn name(self) -> &'static str {
        match self {
            InputOutcome::NotStarted => "not_started",
            InputOutcome::Dispatched => "dispatched",
            InputOutcome::Partial => "partial",
            InputOutcome::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservationOutcome {
    Available,
    Failed,
    Skipped,
}

impl ObservationOutcome {
    pub fn name(self) -> &'static str {
        match self {
            ObservationOutcome::Available => "available",
            ObservationOutcome::Failed => "failed",
            ObservationOutcome::Skipped => "skipped",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupOutcome {
    NotNeeded,
    Released,
    Failed,
    Unknown,
}

impl CleanupOutcome {
    pub fn name(self) -> &'static str {
        match self {
            CleanupOutcome::NotNeeded => "not_needed",
            CleanupOutcome::Released => "released",
            CleanupOutcome::Failed => "failed",
            CleanupOutcome::Unknown => "unknown",
        }
    }
}

/// Truthful record of a step's three independent outcome dimensions.
#[derive(Debug, Clone)]
pub struct StepRecord {
    pub request_id: String,
    pub input_outcome: InputOutcome,
    /// 1-based indexes into the plan of input events that were injected
    /// successfully, in order. Empty for `not_started`/`unknown`.
    pub events_completed: Vec<usize>,
    /// Total input events in the plan.
    pub events_total: usize,
    pub observation_outcome: ObservationOutcome,
    pub cleanup_outcome: CleanupOutcome,
    pub cancelled: bool,
    pub duration_ms: u64,
    pub error: Option<ToolError>,
    /// Observation metadata captured after the step, if any.
    pub observation: Option<ObservationMeta>,
    /// The exact canonical request body (`based_on` + action JSON) this
    /// record was produced for. Compared verbatim on replay — no hashing —
    /// so a noncryptographic collision can never silently change which
    /// stored side effects a replay reports.
    pub request_body: String,
}

impl StepRecord {
    pub fn to_json(&self, image_available: bool) -> Value {
        let mut v = json!({
            "request_id": self.request_id,
            "input_outcome": self.input_outcome.name(),
            "observation_outcome": self.observation_outcome.name(),
            "cleanup_outcome": self.cleanup_outcome.name(),
            "cancelled": self.cancelled,
            "events_completed": self.events_completed,
            "events_total": self.events_total,
            "duration_ms": self.duration_ms,
            "image_available": image_available,
        });
        if let Some(e) = &self.error {
            v["error"] = e.to_json();
        }
        if let Some(obs) = &self.observation {
            let mut o = obs.to_json();
            o["image_available"] = json!(image_available);
            v["observation"] = o;
        }
        v
    }
}

/// Observation metadata, identical whether or not the PNG bytes are still
/// retained in the session image cache.
#[derive(Debug, Clone)]
pub struct ObservationMeta {
    pub observation_id: String,
    pub session_id: String,
    pub surface_id: String,
    pub geometry_version: String,
    pub input_sequence: u64,
    pub width_px: u32,
    pub height_px: u32,
    pub settled: bool,
    pub display_metadata: Value,
}

impl ObservationMeta {
    pub fn to_json(&self) -> Value {
        let mut data = json!({
            "observation_id": self.observation_id,
            "session_id": self.session_id,
            "surface_id": self.surface_id,
            "geometry_version": self.geometry_version,
            "input_sequence": self.input_sequence,
            "settled": self.settled,
            // CONTRACT: image dimensions live at the observation root.
            "width_px": self.width_px,
            "height_px": self.height_px,
            "image": {
                "mime_type": "image/png",
                "width_px": self.width_px,
                "height_px": self.height_px,
            },
        });
        if let Some(fields) = self.display_metadata.as_object() {
            for (k, v) in fields {
                data[k] = v.clone();
            }
        }
        data
    }
}

/// One captured observation: metadata plus the image bytes (single-counted in
/// the session image cache) plus everything needed to validate and map future
/// steps against it.
#[derive(Debug, Clone)]
pub struct Observation {
    pub meta: ObservationMeta,
    pub captured_at: Instant,
    pub png: Vec<u8>,
    pub map: CoordMap,
    pub regions: Option<Arc<rpa_display_topology::ObservationMapping>>,
}

impl Observation {
    /// Metadata exactly matching `Reply.image_png` bytes.
    pub fn to_json(&self) -> Value {
        self.meta.to_json()
    }

    fn image_key(&self) -> String {
        self.meta.observation_id.clone()
    }
}

/// A request registered in the ledger before any input is dispatched.
#[derive(Debug, Clone)]
struct RequestEntry {
    /// Exact canonical body: `based_on` + '\n' + serialized action JSON.
    body: String,
}

fn canonical_body(based_on: &str, action: &Value) -> String {
    format!(
        "{}\n{}",
        based_on,
        serde_json::to_string(action).unwrap_or_default()
    )
}

/// All retained PNGs, accounted in exactly one place. Each entry also keeps
/// the observation's metadata and coordinate map so the previous observation
/// can still serve as a step basis even after its image bytes were evicted.
/// Observations referenced by step records share entries with the
/// current/previous observation.
#[derive(Default)]
struct ImageCache {
    images: HashMap<String, CachedObservation>,
    /// Insertion order; eviction walks from the front. The most recent two
    /// observations (current and previous) are protected from eviction.
    order: VecDeque<String>,
    bytes: u64,
    /// observation_id of the current and previous observation, protected.
    protected: Vec<String>,
}

/// One retained observation entry: bytes are the budgeted part; metadata and
/// the coordinate map stay for basis resolution even if the bytes were
/// evicted. Metadata is unbudgeted (bounded by request/observation counts,
/// trivially small next to PNG bytes).
struct CachedObservation {
    png: Vec<u8>,
    meta: ObservationMeta,
    captured_at: Instant,
    map: CoordMap,
    regions: Option<Arc<rpa_display_topology::ObservationMapping>>,
}

impl ImageCache {
    /// Insert an observation under `key`. Entry metadata/map are replaced on
    /// re-insert; the byte delta accounts the PNG exactly once.
    fn insert(&mut self, key: String, obs: &Observation, max_bytes: u64) -> Result<(), ToolError> {
        let incoming = obs.png.len() as u64;
        if incoming > max_bytes {
            return Err(ToolError::new(
                codes::RESOURCE_LIMIT,
                "single observation image exceeds the per-session image memory budget",
            ));
        }
        // Evict oldest unprotected images until the new one fits.
        while self.bytes + incoming > max_bytes {
            let Some(oldest) = self.order.front().cloned() else {
                break;
            };
            if self.protected.contains(&oldest) {
                // Try the next one; if everything is protected we cannot fit.
                if self.order.iter().all(|k| self.protected.contains(k)) {
                    return Err(ToolError::new(
                        codes::RESOURCE_LIMIT,
                        "image memory budget exhausted by protected observations",
                    ));
                }
                self.order.rotate_left(1);
                continue;
            }
            self.order.pop_front();
            if let Some(entry) = self.images.remove(&oldest) {
                self.bytes -= entry.png.len() as u64;
            }
        }
        self.bytes += incoming;
        self.order.push_back(key.clone());
        self.images.insert(
            key,
            CachedObservation {
                png: obs.png.clone(),
                meta: obs.meta.clone(),
                captured_at: obs.captured_at,
                map: obs.map,
                regions: obs.regions.clone(),
            },
        );
        Ok(())
    }

    fn protect(&mut self, key: String) {
        self.protected.push(key);
        if self.protected.len() > 2 {
            self.protected.remove(0);
        }
    }

    fn get(&self, key: &str) -> Option<Vec<u8>> {
        self.images.get(key).map(|e| e.png.clone())
    }

    /// Reconstruct a full observation from a retained entry (metadata and map
    /// are kept even for entries whose bytes were evicted). Used to validate a
    /// step based on the immediately previous, still-fresh observation.
    fn observation(&self, key: &str) -> Option<Observation> {
        self.images.get(key).map(|e| Observation {
            meta: e.meta.clone(),
            captured_at: e.captured_at,
            png: e.png.clone(),
            map: e.map,
            regions: e.regions.clone(),
        })
    }

    fn contains(&self, key: &str) -> bool {
        self.images.contains_key(key)
    }

    /// Evict the oldest unprotected images beyond `keep` result images.
    fn trim_unprotected(&mut self, keep: usize) {
        let unprotected = self.order.len().saturating_sub(self.protected.len());
        let mut to_evict = unprotected.saturating_sub(keep);
        while to_evict > 0 {
            let Some(oldest) = self.order.front().cloned() else {
                break;
            };
            if self.protected.contains(&oldest) {
                self.order.rotate_left(1);
                // Avoid infinite loop when everything up front is protected.
                if self
                    .order
                    .iter()
                    .take(to_evict + self.protected.len())
                    .all(|k| self.protected.contains(k))
                {
                    break;
                }
                continue;
            }
            self.order.pop_front();
            if let Some(entry) = self.images.remove(&oldest) {
                self.bytes -= entry.png.len() as u64;
            }
            to_evict -= 1;
        }
    }
}

pub struct Session {
    pub id: String,
    pub state: SessionState,
    max_image: (u32, u32),
    pub(crate) display_generation: Option<rpa_display_topology::Generation>,
    pub(crate) display_selection: rpa_display_topology::Selection,
    geometry: Geometry,
    input_sequence: u64,
    observation_counter: u64,
    current_observation: Option<Observation>,
    /// observation_id of the observation before the current one, if retained.
    last_observation_id: Option<String>,
    /// Owned reconstruction of the retained previous observation, populated
    /// when a step validates against it (see `resolve_basis`). Lives on the
    /// session so the validated basis can be returned as a reference.
    retained_previous: Option<Observation>,
    /// Single-counted store for every retained PNG.
    images: ImageCache,
    /// Byte budget for the image cache (test-controllable).
    image_byte_budget: u64,
    requests: HashMap<String, RequestEntry>,
    /// Full outcome metadata for EVERY registered request — never evicted.
    results: HashMap<String, StepRecord>,
    /// Insertion order for image-only trimming.
    result_order: VecDeque<String>,
}

impl Session {
    pub fn new(id: String, geometry: Geometry, max_image: (u32, u32)) -> Self {
        Self {
            id,
            state: SessionState::Ready,
            max_image,
            display_generation: None,
            display_selection: rpa_display_topology::Selection::Primary,
            geometry,
            input_sequence: 0,
            observation_counter: 0,
            current_observation: None,
            last_observation_id: None,
            retained_previous: None,
            images: ImageCache::default(),
            image_byte_budget: MAX_IMAGE_BYTES_PER_SESSION,
            requests: HashMap::new(),
            results: HashMap::new(),
            result_order: VecDeque::new(),
        }
    }

    /// Test seam: shrink the image byte budget without touching production
    /// defaults.
    #[cfg(test)]
    pub fn set_image_byte_budget(&mut self, budget: u64) {
        self.image_byte_budget = budget;
    }

    #[cfg(test)]
    fn image_bytes(&self) -> u64 {
        self.images.bytes
    }

    pub fn surface_id(&self) -> &str {
        &self.geometry.surface_id
    }

    /// Invalidate every stored observation basis. Decisions made while the
    /// session was paused are discarded: the next step must be based on a
    /// fresh observation. Cached request results and their images are
    /// untouched — get_step/replay of prior requests stays intact.
    pub fn invalidate_observations(&mut self) {
        self.current_observation = None;
        self.last_observation_id = None;
        self.retained_previous = None;
    }

    /// The geometry version this session is currently bound to.
    pub fn geometry_version(&self) -> &str {
        &self.geometry.version
    }

    /// The full geometry (identity, origin, size, and version) this session
    /// is bound to; compared whole on resume, never field-picked.
    pub fn geometry_ref(&self) -> &Geometry {
        &self.geometry
    }

    fn next_observation_id(&mut self) -> String {
        self.observation_counter += 1;
        format!("{}-obs-{}", self.id, self.observation_counter)
    }

    /// Store an observation image in the single-counted cache. The previous
    /// current image loses its protection (it becomes the "last" one) and may
    /// later be evicted; metadata outside the cache is unaffected.
    fn store_observation(&mut self, obs: Observation) -> Result<(), ToolError> {
        let key = obs.image_key();
        self.images
            .insert(key.clone(), &obs, self.image_byte_budget)?;
        self.images.protect(key.clone());
        self.last_observation_id = self.current_observation.as_ref().map(|o| o.image_key());
        self.current_observation = Some(obs);
        // A new current observation supersedes any reconstructed basis.
        self.retained_previous = None;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Observation capture
// ---------------------------------------------------------------------------

/// Capture a fresh observation. On geometry change the session faults and the
/// error is `geometry_changed`; the caller must not pretend the old geometry
/// still applies.
mod observation;
pub use observation::capture_observation;

fn fault(session: &mut Session, err: ToolError) -> ToolError {
    session.state = SessionState::Faulted;
    err
}

/// Sleep in small slices so cancellation is honored promptly. Returns
/// `cancelled` if the flag is set mid-sleep (or was already set).
pub fn sleep_cancellable(d: Duration, cancel: &Arc<AtomicBool>) -> Result<(), ToolError> {
    let start = Instant::now();
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err(ToolError::new(codes::CANCELLED, "operation cancelled"));
        }
        let elapsed = start.elapsed();
        if elapsed >= d {
            return Ok(());
        }
        let remaining = d - elapsed;
        std::thread::sleep(remaining.min(CANCEL_CHECK_SLICE));
    }
}

// ---------------------------------------------------------------------------
// Step execution
// ---------------------------------------------------------------------------

pub struct StepContext<'a> {
    pub backend: &'a mut dyn Backend,
    pub cancel: Arc<AtomicBool>,
    /// Execution configuration (test seam; production uses
    /// `ExecutionConfig::production()`).
    pub config: execute::ExecutionConfig,
}

/// Execute a step: validate, dispatch input, settle, observe, record.
/// Never panics; every failure path produces a truthful `StepRecord`.
pub fn execute_step(
    session: &mut Session,
    ctx: &mut StepContext,
    request_id: &str,
    based_on: &str,
    action_value: &Value,
) -> StepRecord {
    let started = Instant::now();
    let body = canonical_body(based_on, action_value);
    let mut record = StepRecord {
        request_id: request_id.to_string(),
        input_outcome: InputOutcome::NotStarted,
        events_completed: Vec::new(),
        events_total: 0,
        observation_outcome: ObservationOutcome::Skipped,
        cleanup_outcome: CleanupOutcome::NotNeeded,
        cancelled: false,
        duration_ms: 0,
        error: None,
        observation: None,
        request_body: body.clone(),
    };

    // --- dedup: idempotent lookup happens BEFORE any stale validation ------
    // Full outcome metadata is retained for every registered request, so a
    // matching entry always replays its stored result — never "unknown".
    if let Some(entry) = session.requests.get(request_id) {
        if entry.body == body {
            if let Some(prior) = session.results.get(request_id) {
                return prior.clone();
            }
            // Unreachable (every registered request stores its result), but
            // never blindly re-execute if it somehow happens.
            record.input_outcome = InputOutcome::Unknown;
            record.cleanup_outcome = CleanupOutcome::Unknown;
            record.error = Some(ToolError::new(
                codes::REQUEST_CONFLICT,
                "request was registered earlier but its result record is missing; \
                 previous input effects are unknown — re-observe and use a new request_id",
            ));
            record.duration_ms = started.elapsed().as_millis() as u64;
            return record;
        }
        record.error = Some(ToolError::new(
            codes::REQUEST_CONFLICT,
            format!("request_id {request_id} was already used with a different request body"),
        ));
        record.duration_ms = started.elapsed().as_millis() as u64;
        return record;
    }

    // --- validate everything before touching the backend -------------------
    let validated = validate_step(session, ctx.backend, based_on, action_value);
    let (action, basis) = match validated {
        Ok(v) => v,
        Err(e) => {
            record.error = Some(e);
            record.duration_ms = started.elapsed().as_millis() as u64;
            return record;
        }
    };

    // Register the request only once we know it can, in principle, run —
    // but crucially BEFORE any input is dispatched.
    if session.requests.len() >= MAX_REQUESTS_PER_SESSION {
        record.error = Some(ToolError::new(
            codes::RESOURCE_LIMIT,
            format!("session reached {MAX_REQUESTS_PER_SESSION} registered requests"),
        ));
        record.duration_ms = started.elapsed().as_millis() as u64;
        return record;
    }
    session
        .requests
        .insert(request_id.to_string(), RequestEntry { body: body.clone() });

    // Prevalidate every key/button name BEFORE any event is injected. The
    // contract backend validates names before pressing anything, but for a
    // chord an invalid later name could otherwise leave earlier modifiers
    // pressed; strict vocabulary validation here is the enforcement point.
    let (keys, buttons) = action.names();
    if let Err(e) = prevalidate_names(&keys, &buttons) {
        record.error = Some(e);
        record.duration_ms = started.elapsed().as_millis() as u64;
        unregister(session, request_id);
        return record;
    }

    // Any step that passes validation invalidates prior observations even if
    // dispatch later fails partway: input may have reached the desktop.
    session.input_sequence += 1;

    // --- dispatch (generic executor: chunked text, deadlines, releases) ----
    let outcome = crate::runtime::display_input::execute(
        &action,
        &basis,
        ctx.backend,
        &ctx.cancel,
        &ctx.config,
        drag_move_count(&action),
    );
    record.input_outcome = outcome.input_outcome;
    record.events_completed = outcome.events_completed;
    record.events_total = outcome.events_total;
    record.cleanup_outcome = outcome.cleanup_outcome;
    record.cancelled = outcome.cancelled;
    record.error = outcome.error;
    if record.cleanup_outcome == CleanupOutcome::Failed
        || record
            .error
            .as_ref()
            .is_some_and(|e| e.code == codes::GEOMETRY_CHANGED)
    {
        // Cleanup failure faults the session: input state cannot be trusted.
        session.state = SessionState::Faulted;
    }

    // --- observation --------------------------------------------------------
    // A screenshot failure never erases a dispatched input outcome.
    if session.state == SessionState::Faulted {
        record.observation_outcome = ObservationOutcome::Skipped;
    } else {
        match capture_observation(session, ctx.backend, 0, &ctx.cancel) {
            Ok(mut obs) => {
                obs.meta.settled = outcome.settled;
                record.observation_outcome = ObservationOutcome::Available;
                record.observation = Some(obs.meta.clone());
            }
            Err(e) => {
                record.observation_outcome = ObservationOutcome::Failed;
                if record.error.is_none() {
                    record.error = Some(e);
                }
            }
        }
    }

    if ctx.cancel.load(Ordering::SeqCst) && !record.cancelled {
        // Cancellation arrived after dispatch finished (e.g. during observe).
        record.cancelled = true;
    }

    record.duration_ms = started.elapsed().as_millis() as u64;
    store_result(session, record.clone());
    record
}

/// Remove a request that was registered but then proven unable to run before
/// any input (name prevalidation failure), so a corrected request with the
/// same id is not a false conflict.
fn unregister(session: &mut Session, request_id: &str) {
    session.requests.remove(request_id);
}

/// Strict up-front name validation. Parses produced these names, so this is
/// defense in depth; keeping it separate from `validate_step` makes the
/// "no input for invalid combos" guarantee explicit and testable.
fn prevalidate_names(keys: &[String], buttons: &[String]) -> Result<(), ToolError> {
    for k in keys {
        actions::parse_key_name(Some(k))?;
    }
    for b in buttons {
        if !matches!(b.as_str(), "left" | "right" | "middle") {
            return Err(ToolError::new(
                codes::INVALID_ACTION,
                format!("unknown button name: {b}"),
            ));
        }
    }
    Ok(())
}

/// Validate a step. The only backend touch is the pre-input live-geometry
/// re-check: stale observation freshness alone is not enough — a display
/// change must stop the step BEFORE any input is dispatched, faulting the
/// session with a `not_started` record and zero injected events. Returns the
/// action and the observation it is based on.
fn validate_step(
    session: &mut Session,
    backend: &mut dyn Backend,
    based_on: &str,
    action_value: &Value,
) -> Result<(Action, Observation), ToolError> {
    match session.state {
        SessionState::Ready => {}
        SessionState::Paused => {
            return Err(ToolError::new(
                codes::CANCELLED,
                "session is paused; resume before stepping",
            ))
        }
        SessionState::Faulted => {
            return Err(ToolError::new(
                codes::SESSION_STATE,
                "session is faulted; close it and open a new one",
            ))
        }
        SessionState::Closed => {
            return Err(ToolError::new(
                codes::SESSION_NOT_FOUND,
                "session is closed",
            ))
        }
    }

    // Copy the session fields the checks need BEFORE borrowing the basis, so
    // the basis reference (tied to the session borrow) can live alone.
    if let Err(e) = crate::runtime::display::check(backend, session.display_generation) {
        session.state = SessionState::Faulted;
        return Err(e);
    }
    let session_input_sequence = session.input_sequence;
    let session_geometry = session.geometry.clone();

    let basis = resolve_basis(session, based_on)?;

    if basis.meta.input_sequence != session_input_sequence {
        return Err(ToolError::new(
            codes::STALE_OBSERVATION,
            "observation predates the latest input; re-observe",
        ));
    }
    if basis.captured_at.elapsed() > OBSERVATION_FRESHNESS {
        return Err(ToolError::new(
            codes::STALE_OBSERVATION,
            format!(
                "observation is older than {}s; re-observe",
                OBSERVATION_FRESHNESS.as_secs()
            ),
        ));
    }

    // Pre-input live-geometry revalidation: coordinates were computed from
    // the stored observation's geometry. If the display geometry changed
    // since (version, surface identity, origin, or size), dispatching now
    // would target wrong native coordinates. Fault the session; nothing has
    // been injected yet, so the record stays `not_started` with zero events.
    // The basis borrow ends before the session is faulted (`basis` is used
    // no further on either error path).
    let geometry = match backend.geometry() {
        Ok(g) => g,
        Err(e) => {
            let err = ToolError::from(e);
            session.state = SessionState::Faulted;
            return Err(err);
        }
    };
    if geometry != session_geometry {
        let err = ToolError::new(
            codes::GEOMETRY_CHANGED,
            format!(
                "display geometry changed since the observation was captured (was {}, now {})",
                session_geometry.version, geometry.version
            ),
        );
        session.state = SessionState::Faulted;
        return Err(err);
    }

    let action = actions::parse_action(action_value)?;
    for pos in action.positions() {
        if !crate::runtime::display_input::contains(&basis, pos) {
            return Err(ToolError::new(
                codes::INVALID_ACTION,
                format!(
                    "position {:?} is outside the {}x{} image it must refer to",
                    pos, basis.meta.width_px, basis.meta.height_px
                ),
            ));
        }
    }
    Ok((action, basis.clone()))
}

/// Resolve the observation a step is based on. The current observation is the
/// normal basis. A read-only re-observe does not invalidate anything (it never
/// injects input), so the immediately preceding observation remains an equally
/// truthful basis — provided its input sequence still matches the session's.
/// Any input since it was captured bumps the sequence and the input-sequence
/// check in `validate_step` rejects it, preserving "input invalidates prior
/// observation". Both are downscaled copies of the same live geometry, so
/// their coordinate maps are identical.
fn resolve_basis<'a>(
    session: &'a mut Session,
    based_on: &str,
) -> Result<&'a Observation, ToolError> {
    let current = session.current_observation.as_ref().ok_or_else(|| {
        ToolError::new(
            codes::STALE_OBSERVATION,
            "no observation yet; call computer_observe first",
        )
    })?;
    if based_on == current.meta.observation_id {
        return Ok(current);
    }
    // Scope the owned reconstruction so `current` (the borrowed basis) can
    // be returned without conflicting borrows.
    let previous_retained = session.last_observation_id.as_deref() == Some(based_on)
        && session.images.contains(based_on);
    if previous_retained {
        session.retained_previous = session.images.observation(based_on);
        if let Some(previous) = session.retained_previous.as_ref() {
            return Ok(previous);
        }
    }
    Err(ToolError::new(
        codes::STALE_OBSERVATION,
        format!(
            "based_on {based_on} is not the current observation {}; re-observe",
            current.meta.observation_id
        ),
    ))
}

fn drag_move_count(action: &Action) -> usize {
    match action {
        Action::Drag { duration_ms, .. } => {
            ((*duration_ms / actions::DRAG_MOVE_INTERVAL_MS) as usize).clamp(2, 600)
        }
        _ => 1,
    }
}

/// Store a step result. Metadata is retained for every registered request
/// (bounded by MAX_REQUESTS_PER_SESSION, which rejects new steps first);
/// only the images of older results are trimmed.
fn store_result(session: &mut Session, record: StepRecord) {
    if session.results.contains_key(&record.request_id) {
        return;
    }
    session.result_order.push_back(record.request_id.clone());
    session.results.insert(record.request_id.clone(), record);
    // Trim image bytes of older results (metadata untouched, replay-safe).
    session
        .images
        .trim_unprotected(MAX_RESULT_IMAGES_PER_SESSION);
}

/// Public helpers used by the Runtime facade.
impl Session {
    pub fn get_result(&self, request_id: &str) -> Result<Value, ToolError> {
        if let Some(record) = self.results.get(request_id) {
            let image_available = record
                .observation
                .as_ref()
                .map(|m| self.images.contains(&m.observation_id))
                .unwrap_or(false);
            return Ok(record.to_json(image_available));
        }
        if self.requests.contains_key(request_id) {
            return Err(ToolError::new(
                codes::REQUEST_NOT_FOUND,
                "request was registered but its result record is missing; \
                 its input effects are unknown — re-observe before continuing",
            ));
        }
        Err(ToolError::new(
            codes::REQUEST_NOT_FOUND,
            format!("no request with id {request_id} in this session"),
        ))
    }

    /// Image bytes for a result, only if still retained AND the record's
    /// observation actually captured an image. Absence is surfaced through
    /// `image_available: false` in `get_result`.
    pub fn result_image(&self, request_id: &str) -> Option<Vec<u8>> {
        let record = self.results.get(request_id)?;
        record.observation.as_ref()?;
        // A record whose observation phase produced no image must never
        // borrow bytes captured for a different observation.
        if record.observation_outcome != ObservationOutcome::Available {
            return None;
        }
        self.result_image_for(record)
    }

    /// Image bytes bound to THIS record's own observation, looked up by the
    /// observation_id the record itself produced. A freshly computed error
    /// record (request conflict, invalid request, ...) carries no
    /// observation, so it can never borrow a prior image stored under the
    /// same request_id — unlike a bare `result_image(request_id)` lookup,
    /// which is only correct for the stored record.
    pub fn result_image_for(&self, record: &StepRecord) -> Option<Vec<u8>> {
        let meta = record.observation.as_ref()?;
        if record.observation_outcome != ObservationOutcome::Available {
            return None;
        }
        self.images.get(&meta.observation_id)
    }
}

/// Capabilities advertised by `computer_describe`/`computer_open`.
pub fn capabilities(platform: &str) -> Value {
    json!({
        "platform": platform,
        "actions": [
            "click", "move", "drag", "scroll", "text_input", "key_chord", "key_hold"
        ],
        "limits": {
            "click_count_max": actions::CLICK_MAX_COUNT,
            "drag_points": [actions::DRAG_MIN_POINTS, actions::DRAG_MAX_POINTS],
            "drag_duration_ms_max": actions::DRAG_MAX_DURATION_MS,
            "text_chars_max": actions::TEXT_MAX_CHARS,
            "scroll_ticks_max": actions::SCROLL_MAX_TICKS,
            "scroll_unit": "wheel_ticks",
            "key_hold_ms_max": actions::HOLD_MAX_DURATION_MS,
            "observation_freshness_s": OBSERVATION_FRESHNESS.as_secs(),
            "observe_wait_ms_max": OBSERVE_MAX_WAIT_MS,
            "step_deadline_s": STEP_DEADLINE.as_secs(),
            "max_image_px": MAX_OPEN_DIMENSION,
            "requests_per_session_max": MAX_REQUESTS_PER_SESSION,
        },
        "coordinate_space": "image_pixels_top_left_origin",
        "image_format": "png",
    })
}

#[cfg(test)]
mod tests;
