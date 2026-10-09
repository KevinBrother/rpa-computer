//! Synthetic native boundary only; parsing, planning and cancellation remain production code.
use rpa_computer::backend::{Backend, BackendError, Capture, Direction, Geometry, InputEvent};
use rpa_computer::mcp::{backend_factory::BackendFactory, server::McpService, worker::Worker};
use rpa_computer::runtime::Reply;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub const BOUND: Duration = Duration::from_secs(8);
#[derive(Default, Debug)]
pub struct Trace {
    pub events: Vec<InputEvent>,
    pub held: BTreeSet<String>,
    pub cleanups: usize,
    pub fail_cleanup: bool,
    pub gate_timeout: bool,
}
#[derive(Clone, Copy)]
pub enum Point {
    ButtonPress,
    DragMove,
    KeyPress,
    Text,
}
struct Fake {
    trace: Arc<Mutex<Trace>>,
    point: Point,
    entered: Option<mpsc::Sender<()>>,
    release: mpsc::Receiver<()>,
}
fn geometry() -> Geometry {
    Geometry {
        surface_id: "cancel-fixture".into(),
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
    fn inject(&mut self, event: &InputEvent) -> Result<(), BackendError> {
        // Record the actual dispatch and its synthetic physical side effect BEFORE blocking.
        // No cancel flag or held-state mutation is performed by the barrier.
        let hit = {
            let mut t = self.trace.lock().unwrap();
            match event {
                InputEvent::Button {
                    button, direction, ..
                } => update(&mut t.held, format!("button:{button}"), *direction),
                InputEvent::Key { key, direction } => {
                    update(&mut t.held, format!("key:{key}"), *direction)
                }
                _ => {}
            }
            t.events.push(event.clone());
            match self.point {
                Point::ButtonPress => matches!(
                    event,
                    InputEvent::Button {
                        direction: Direction::Press,
                        ..
                    }
                ),
                Point::DragMove => {
                    matches!(event, InputEvent::Move { .. }) && t.held.contains("button:left")
                }
                Point::KeyPress => matches!(
                    event,
                    InputEvent::Key {
                        direction: Direction::Press,
                        ..
                    }
                ),
                Point::Text => matches!(event, InputEvent::Text { .. }),
            }
        };
        if hit {
            if let Some(entered) = self.entered.take() {
                let _ = entered.send(());
                if self.release.recv_timeout(BOUND).is_err() {
                    self.trace.lock().unwrap().gate_timeout = true;
                    return Err(BackendError::new(
                        "barrier_timeout",
                        "supervisor did not release dispatch",
                    ));
                }
            }
        }
        Ok(())
    }
    fn release_all(&mut self) -> Result<(), BackendError> {
        let mut t = self.trace.lock().unwrap();
        t.cleanups += 1;
        if t.fail_cleanup {
            return Err(BackendError::new(
                "input_failed",
                "synthetic release_all failure",
            ));
        }
        t.held.clear();
        Ok(())
    }
}
fn update(held: &mut BTreeSet<String>, name: String, direction: Direction) {
    match direction {
        Direction::Press => {
            held.insert(name);
        }
        Direction::Release => {
            held.remove(&name);
        }
    }
}
/// Unconditionally releases on panic. Dropping before Worker guarantees no blocked fake call
/// outlives teardown. Timeout is a test failure, NEVER a successful cancellation oracle.
pub struct Release(Option<mpsc::Sender<()>>);
impl Release {
    pub fn now(&mut self) {
        if let Some(tx) = self.0.take() {
            let _ = tx.send(());
        }
    }
}
impl Drop for Release {
    fn drop(&mut self) {
        self.now();
    }
}
pub struct Job<T> {
    rx: mpsc::Receiver<T>,
    join: Option<JoinHandle<()>>,
}
impl<T: Send + 'static> Job<T> {
    pub fn spawn(f: impl FnOnce() -> T + Send + 'static) -> Self {
        let (tx, rx) = mpsc::channel();
        let join = thread::spawn(move || {
            let result = f();
            let _ = tx.send(result);
        });
        Self {
            rx,
            join: Some(join),
        }
    }
    pub fn finish(mut self) -> T {
        // Never unconditionally join a potentially stuck production call. On timeout Drop
        // detaches; fake backend itself has a hard bound and Release wakes it on unwind.
        let result = self
            .rx
            .recv_timeout(BOUND)
            .expect("bounded production job reply");
        self.join
            .take()
            .unwrap()
            .join()
            .expect("production job panicked");
        result
    }
}
pub struct Harness {
    pub worker: Arc<Worker>,
    pub trace: Arc<Mutex<Trace>>,
    pub entered: mpsc::Receiver<()>,
    pub release: Release,
    pub sid: String,
    pub base: String,
}
impl Harness {
    pub fn new(point: Point, fail: bool) -> Self {
        let trace = Arc::new(Mutex::new(Trace {
            fail_cleanup: fail,
            ..Trace::default()
        }));
        let (entered_tx, entered) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let shared = trace.clone();
        let worker = Arc::new(
            Worker::start(BackendFactory::Test(Box::new(move || {
                Ok(Box::new(Fake {
                    trace: shared,
                    point,
                    entered: Some(entered_tx),
                    release: release_rx,
                }))
            })))
            .unwrap(),
        );
        let open = call(&worker, "computer_open", json!({}));
        assert!(!open.is_error, "{:?}", open.data);
        let sid = open.data["session_id"].as_str().unwrap().to_string();
        let obs = call(&worker, "computer_observe", json!({"session_id": sid}));
        assert!(!obs.is_error, "{:?}", obs.data);
        let base = obs.data["observation_id"].as_str().unwrap().to_string();
        Self {
            worker,
            trace,
            entered,
            release: Release(Some(release_tx)),
            sid,
            base,
        }
    }
    pub fn args(&self, id: &str, action: Value) -> Value {
        json!({"session_id": self.sid, "request_id": id, "based_on": self.base, "action": action})
    }
    pub fn start(&self, action: Value) -> Job<Reply> {
        let worker = self.worker.clone();
        let args = self.args("partial", action);
        // MCP service owns the real request ledger (wire id 101, runtime id partial).
        Job::spawn(move || {
            let mut service = McpService::new(&worker, "cancel-contract".into());
            service.handle_line(
                &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}).to_string(),
            );
            service.handle_line(
                &json!({"jsonrpc":"2.0","method":"notifications/initialized"}).to_string(),
            );
            let (frames, _) = service.handle_line(&json!({"jsonrpc":"2.0","id":101,"method":"tools/call","params":{"name":"computer_step","arguments":args}}).to_string());
            let wire: Value = serde_json::from_str(&frames[0]).unwrap();
            let result = &wire["result"];
            let data = serde_json::from_str(
                result["content"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|block| block["type"] == "text")
                    .unwrap()["text"]
                    .as_str()
                    .unwrap(),
            )
            .unwrap();
            Reply {
                data,
                image_png: None,
                is_error: result["isError"].as_bool().unwrap(),
            }
        })
    }
    pub fn at_barrier(&self) -> Vec<InputEvent> {
        self.entered
            .recv_timeout(BOUND)
            .expect("real backend dispatch barrier");
        assert!(!self.worker.cancel_handle().is_cancelled());
        self.trace.lock().unwrap().events.clone()
    }
    pub fn control(&self, name: &'static str) -> Job<Reply> {
        let worker = self.worker.clone();
        let args = json!({"session_id":self.sid});
        Job::spawn(move || call(&worker, name, args))
    }
    pub fn await_cancel(&self) {
        let deadline = Instant::now() + BOUND;
        while !self.worker.cancel_handle().is_cancelled() {
            assert!(
                Instant::now() < deadline,
                "control serialized behind blocked native dispatch"
            );
            thread::yield_now();
        }
    }
    pub fn assert_tail(&self, prefix: &[InputEvent], cleanup_failed: bool) {
        let t = self.trace.lock().unwrap();
        assert!(!t.gate_timeout);
        assert_eq!(&t.events[..prefix.len()], prefix);
        assert!(
            t.events[prefix.len()..].iter().all(|e| matches!(
                e,
                InputEvent::Button {
                    direction: Direction::Release,
                    ..
                } | InputEvent::Key {
                    direction: Direction::Release,
                    ..
                }
            )),
            "business input after cancellation: {:?}",
            t.events
        );
        assert_eq!(
            t.held.is_empty(),
            !cleanup_failed,
            "synthetic physical held-state: {t:?}"
        );
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        self.release.now();
    }
}
pub fn call(worker: &Worker, name: &str, args: Value) -> Reply {
    worker.call_with_deadline(name, args, BOUND).unwrap()
}
pub fn code(reply: &Reply) -> &str {
    assert!(reply.is_error, "{:?}", reply.data);
    reply.data["error"]["code"]
        .as_str()
        .or_else(|| reply.data["code"].as_str())
        .unwrap()
}
pub fn partial(reply: &Reply, completed: usize, cleanup: &str) {
    assert!(
        reply.is_error,
        "cancel must not claim success: {:?}",
        reply.data
    );
    assert_eq!(reply.data["input_outcome"], "partial");
    assert_eq!(reply.data["cancelled"], true);
    assert_eq!(reply.data["cleanup_outcome"], cleanup);
    assert_eq!(
        reply.data["events_completed"].as_array().unwrap().len(),
        completed
    );
    assert!(reply.data["events_total"].as_u64().unwrap() > completed as u64);
    assert_eq!(
        code(reply),
        if cleanup == "failed" {
            "input_error"
        } else {
            "cancelled"
        }
    );
    assert_eq!(
        reply.data["events_completed"],
        json!((0..completed).collect::<Vec<_>>())
    );
}
pub fn drag() -> Value {
    json!({"kind":"drag","path":[[0,0],[8,8],[15,15]],"duration_ms":16})
}
pub fn hold() -> Value {
    json!({"kind":"key_hold","key":"shift","duration_ms":1000})
}
