use super::super::native::lifecycle::{bit, ChildPoint, ParentPoint, Probe};
use super::harness::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::time::{Duration, Instant};

const MAX_CHURN_THREADS: usize = 4096;
const CHURN_BUDGET: Duration = Duration::from_secs(2);
struct Replacement {
    stop: Arc<AtomicBool>,
    thread: std::thread::JoinHandle<()>,
}
impl Replacement {
    fn finish(self) {
        self.stop.store(true, Ordering::Release);
        assert!(
            wait(Instant::now() + Duration::from_millis(300), || self
                .thread
                .is_finished()),
            "ADMISSION_FIXTURE_INFRA: replacement thread did not stop"
        );
        self.thread
            .join()
            .expect("ADMISSION_FIXTURE_INFRA: replacement thread panic");
    }
}

// At most ONE churn/replacement thread is live at a time; each nonmatch is joined
// before another spawn. No PID allocator writes, namespaces, credentials or GO.
fn real_replacement(target: i32, hold_until: Instant) -> (Option<Replacement>, usize) {
    let until = Instant::now() + CHURN_BUDGET;
    let mut count = 0;
    while count < MAX_CHURN_THREADS && Instant::now() < until {
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let (tx, rx) = mpsc::sync_channel(1);
        let thread = std::thread::Builder::new()
            .stack_size(128 * 1024)
            .spawn(move || {
                let tid = unsafe { libc::syscall(libc::SYS_gettid) } as i32;
                let group = unsafe { libc::syscall(libc::SYS_getpid) } as i32;
                let _ = tx.send((tid, group));
                if tid == target {
                    while !stopping.load(Ordering::Acquire) && Instant::now() < hold_until {
                        std::thread::sleep(Duration::from_millis(1));
                    }
                }
            })
            .expect("ADMISSION_FIXTURE_INFRA: bounded churn spawn");
        count += 1;
        let (tid, group) = rx
            .recv_timeout(
                until
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(300)),
            )
            .expect("ADMISSION_FIXTURE_INFRA: churn identity not delivered");
        assert!(
            tid > 0 && group == std::process::id() as i32,
            "ADMISSION_FIXTURE_INFRA: kernel thread identity"
        );
        let replacement = Replacement { stop, thread };
        if tid == target {
            assert!(
                !replacement.thread.is_finished(),
                "ADMISSION_FIXTURE_INFRA: replacement not alive"
            );
            return (Some(replacement), count);
        }
        replacement.finish();
    }
    (None, count)
}

pub(super) fn run(require_reuse: bool) {
    let file = Authority::new();
    let exits = ExitObserver::install();
    let point = ParentPoint::CloneReturned;
    let probe = Probe::new(bit(point as u32), bit(ChildPoint::BeforePrctl as u32));
    let deadline = Instant::now() + Duration::from_secs(5);
    let worker = Worker::start(&file, probe.clone(), deadline);
    let setup = Instant::now() + Duration::from_millis(700);
    parent(&probe, point, setup);
    child(&probe, ChildPoint::BeforePrctl, setup);
    let observer = Observer::from_probe(&probe);
    let pid = probe.raw().child_pid.load(Ordering::Acquire);
    assert!(pid > 0);
    let original_tid = worker.raw_exit(&probe, point);
    // Numeric ID is only used to demand actual kernel reuse in the FIXTURE.
    // Neither admission nor product cleanup is authorized by this comparison.
    let (replacement, count) = if require_reuse {
        real_replacement(original_tid, deadline + Duration::from_secs(1))
    } else {
        (None, 0)
    };
    if require_reuse && replacement.is_none() {
        // No child release / behavioral assertion without the real prerequisite.
        // Observer cleanup is explicit and NOT counted as a successful deadline.
        drop(observer);
        drop(exits);
        drop(file);
        eprintln!("ADMISSION_NOTQUALIFIED: actual same-group TID reuse absent; attempts={count}, max_threads={MAX_CHURN_THREADS}, churn_budget_ms={}, original_tid={original_tid}; NOT PASS/behavior RED", CHURN_BUDGET.as_millis());
        std::process::exit(77);
    }
    assert!(
        Instant::now() + Duration::from_millis(250) < deadline,
        "ADMISSION_FIXTURE_INFRA: no original budget left after prerequisite"
    );
    // Original creator is really gone. A qualifying replacement remains alive,
    // but never receives ARMED or writes GO; no other thread substitutes for it.
    probe.release_child(ChildPoint::BeforePrctl);
    // Pre-prctl death is not retroactively signaled. A legitimate causal
    // ARMED/GO implementation may reject by the original deadline rather than
    // immediately discovering the old thread's death. Do not require a handle.
    let until = deadline + Duration::from_millis(500);
    let exited = wait(until, || observer.exited());
    if exited {
        assert!(
            wait(Instant::now() + Duration::from_millis(200), || exits
                .for_pid(pid)
                .is_some()),
            "ADMISSION_FIXTURE_INFRA: missing kernel termination cause"
        );
    }
    let event = exits.for_pid(pid);
    let path = path_ops(&probe);
    let replacement_alive = replacement.as_ref().map(|r| !r.thread.is_finished());
    println!("ADMISSION_OBSERVATION creator-pre-prctl reuse_required={require_reuse} original_tid={original_tid} matched_tid={} replacement_alive={replacement_alive:?} churn_threads={count} child_pid={pid} exited={exited} event={event:?} path_ops={path:?}", if require_reuse { original_tid } else { 0 });
    // Sampling precedes all fixture signaling/waiting and replacement release.
    drop(observer);
    if let Some(replacement) = replacement {
        replacement.finish();
    }
    if require_reuse {
        assert_eq!(
            replacement_alive,
            Some(true),
            "ADMISSION_FIXTURE_INFRA: reused TID not held alive"
        );
    }
    assert_eq!(
        path,
        (0, 0),
        "ADMISSION_ASSERTION_RED: dead original creator authorized authority work"
    );
    assert!(exited && event.is_some_and(|e| e.pid == pid && e.raw_rejection()),
        "ADMISSION_ASSERTION_RED: dead original creator child failed closed exit requirement; reused={require_reuse}, exited={exited}, event={event:?}");
    // Deliberately no claim that unpublished parent ownership was recovered.
}
