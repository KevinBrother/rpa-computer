# WRITE the one test now; no exploratory reading
Prior exec49655 verified exited1(maxturns12); no test file/report produced. Sole writes tests/transport_lifecycle.rs and own report. No src/Cargo/other test edits, GUI/native Desktop, commit/push. User configured Claude/model unchanged.

Your FIRST meaningful tool must WRITE this single integration test (not source search); enough public API facts below. At most220lines. Then cargo test --test transport_lifecycle --offline -- --nocapture (pipefail), report actual RED/GREEN. If compile errors need source signature, read ONLY that narrow symbol. Do not reread implementation to diagnose known failure; no all-target tests. Expected RED is correct evidence, never weaken assertions.

Imports/known API:
- rpa_computer::backend::{Backend,BackendError,Capture,Geometry,InputEvent}
- rpa_computer::mcp::backend_factory::BackendFactory
- rpa_computer::mcp::worker::Worker
- Backend methods: platform(&self)->&'static str; geometry(&mut self)->Result<Geometry,BackendError>; capture(&mut self)->Result<Capture,BackendError>; inject(&mut self,&InputEvent)->Result<(),BackendError>; release_all(&mut self)->Result<(),BackendError>.
- Geometry { surface_id:String, input_origin:(i32,i32), input_size:(u32,u32), version:String }; use s,(0,0),(16,16),v1.
- Worker::start(BackendFactory::Test(Box::new(move || Ok(Box::new(mock))))) -> Result<Worker,WorkerError>.
- worker.call(name:&str,args:serde_json::Value)->Result<Reply,WorkerError>; Reply {pub is_error:bool,pub data:Value,...}. worker.cancel_handle().cancel(), .is_cancelled(); worker.shutdown().
- call computer_open({}), extract data[session_id] string; call computer_pause({session_id}), then computer_resume({session_id}). Observer code via data[error][code] == "cancelled".

Mock design: Arc AtomicBool armed,entered,release. geometry() normally returns stable Geometry. Once armed.swap(false), entered=true and bounded wait release; no OS functions. capture/inject should panic if called (no such call needed while cancelled; wrong unexpected capture must fail test, not fabricate a screenshot). release_all returns Ok.

Exact scenario:
1. Startworker/open/pause; assert successful setup. armed=true.
2. std::thread::scope closure: create release-on-Drop guard INSIDE closure so it drops BEFORE implicit scoped thread join on panic. Spawn w.call(resume) thread. Bounded poll entered<=3s; then cancel_handle.cancel() while definitely inside geometry; release=true BEFORE joining/retrieving response.
3. Assert after resume completion cancel_handle.is_cancelled() is STILL true (current buggy Runtime.resume clears it AFTER geometry). Assert resume.is_error && error.code==cancelled, never clean ready. Call observe and assert exactcancelled, not capture_error. NEW deliberate resume after gate disarmed must first-call succeed; requires_fresh_observation true. Shutdown.
All waits bounded. No synchronous calls behind unreleasedgate. On current source first stop-flag assertion should fail deterministically. Save exact command/result in .agents/reports/qa-native-resume-race-2.md, don't debug/fix src. Last agent burned budget reading; do not repeat. Stop after report even RED.
