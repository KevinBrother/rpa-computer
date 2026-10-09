//! Actual Backend seam. Never reads stdin, waits for parent release, sets a cancel flag,
//! sleeps, injects OS input, or delegates to DesktopBackend.
use rpa_computer::backend::{Backend, BackendError, Capture, Direction, Geometry, InputEvent};
use rpa_computer::mcp::worker::CancelHandle;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub fn diagnostic(value: Value) -> std::io::Result<()> {
    let mut out = std::io::stderr().lock();
    writeln!(out, "EOF_FIXTURE {}", serde_json::to_string(&value)?)?;
    out.flush()
}
#[derive(Default)]
pub struct Trace {
    pub held: BTreeSet<String>,
    pub events: Vec<Value>,
    pub cleanup: Vec<Value>,
    pub pressed_at: Option<Instant>,
}
impl Trace {
    pub fn json(&self) -> Value {
        json!({"held":self.held,"events":self.events,"cleanup":self.cleanup})
    }
}
pub struct Fake {
    pub trace: Arc<Mutex<Trace>>,
    pub cancel_view: Arc<Mutex<Option<CancelHandle>>>,
}
impl Fake {
    fn cancelled(&self) -> bool {
        self.cancel_view
            .lock()
            .unwrap()
            .as_ref()
            .expect("installed before stdio::run")
            .is_cancelled()
    }
}
fn geometry() -> Geometry {
    Geometry {
        surface_id: "stdio-eof-fake".into(),
        input_origin: (0, 0),
        input_size: (16, 16),
        version: "fixed-1".into(),
    }
}
impl Backend for Fake {
    fn platform(&self) -> &'static str {
        "test"
    }
    fn geometry(&mut self) -> Result<Geometry, BackendError> {
        Ok(geometry())
    }
    fn capture(&mut self) -> Result<Capture, BackendError> {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(16, 16, image::Rgba([7, 8, 9, 255]))
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        Ok(Capture {
            png: bytes.into_inner(),
            width: 16,
            height: 16,
            geometry: geometry(),
        })
    }
    fn inject(&mut self, input: &InputEvent) -> Result<(), BackendError> {
        let mut trace = self.trace.lock().unwrap();
        let (key, direction) = match input {
            InputEvent::Key { key, direction } => (key, direction),
            other => {
                return Err(BackendError::new(
                    "input_failed",
                    format!("unexpected fixture input {other:?}"),
                ))
            }
        };
        match direction {
            Direction::Press => {
                trace.held.insert(key.clone());
                trace.pressed_at = Some(Instant::now());
            }
            Direction::Release => {
                trace.held.remove(key);
            }
        }
        let event = json!({"key":key,"direction":if *direction==Direction::Press {"press"} else {"release"},"cancelled":self.cancelled()});
        trace.events.push(event.clone());
        // Successful synthetic side effect is already recorded. No barrier: production
        // executor now owns the 5000ms hold; the parent can only close its pipe.
        diagnostic(json!({"kind":"dispatch","event":event,"held":trace.held,
            "dispatch_qpc":super::identity::qpc_ticks().map_err(|e|BackendError::new("input_failed",e.to_string()))?}))
            .map_err(|e| BackendError::new("input_failed", e.to_string()))
    }
    fn release_all(&mut self) -> Result<(), BackendError> {
        let mut trace = self.trace.lock().unwrap();
        let before = trace.held.clone();
        trace.held.clear();
        let evidence = json!({"cancelled":self.cancelled(),"held_before":before,"held_after":trace.held,
            "since_press_ms":trace.pressed_at.map(|at|at.elapsed().as_millis() as u64)});
        trace.cleanup.push(evidence.clone());
        diagnostic(json!({"kind":"cleanup","attempt":trace.cleanup.len(),"evidence":evidence}))
            .map_err(|e| BackendError::new("input_failed", e.to_string()))
    }
}
