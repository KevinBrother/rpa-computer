//! Linux process/pipe boundary. X calls are synchronous but bounded by a separate
//! process watchdog: a wedged server cannot keep the renderer falsely healthy.
use super::{process_boundary::*, *};
use rpa_desktop_feedback::{
    protocol::{Ready, RendererMessage},
    Diagnostic,
};
use std::{
    io::{self, Read, Write},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex, MutexGuard, TryLockError,
    },
    thread,
    time::{Duration, Instant},
};
static TERMINATE: AtomicBool = AtomicBool::new(false);
extern "C" fn signal(_: libc::c_int) {
    TERMINATE.store(true, Ordering::SeqCst);
}

struct Watchdog {
    origin: Instant,
    deadline: Arc<AtomicU64>,
}
impl Watchdog {
    fn start() -> Result<Self, UiFailure> {
        let origin = Instant::now();
        let deadline = Arc::new(AtomicU64::new(3000));
        let shared = deadline.clone();
        thread::Builder::new()
            .name("feedback-deadline".into())
            .spawn(move || loop {
                if origin.elapsed().as_millis() as u64 >= shared.load(Ordering::SeqCst) {
                    // No destructors, stdio locks, X synchronization or false cleanup claims.
                    unsafe {
                        libc::_exit(70);
                    }
                }
                thread::sleep(Duration::from_millis(25));
            })
            .map_err(|_| UiFailure::Native)?;
        Ok(Self { origin, deadline })
    }
    fn arm(&self, milliseconds: u64) {
        self.deadline.store(
            self.origin.elapsed().as_millis() as u64 + milliseconds,
            Ordering::SeqCst,
        );
    }
}

struct Fd(libc::c_int);
impl Read for Fd {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        // SAFETY: fd is a validated private pipe/socket; slice is writable for len.
        let n = unsafe { libc::read(self.0, bytes.as_mut_ptr().cast(), bytes.len()) };
        if n < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(n as usize)
        }
    }
}
impl Write for Fd {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        // SAFETY: immutable slice stays live for the one bounded syscall.
        let n = unsafe { libc::write(self.0, bytes.as_ptr().cast(), bytes.len()) };
        if n < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(n as usize)
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn private_pipe(fd: i32) -> Result<(), UiFailure> {
    // SAFETY below: fstat writes the complete stat on success; assume_init is
    // reached only then. fcntl uses integer flags, no variadic pointer arguments.
    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    if unsafe { libc::fstat(fd, stat.as_mut_ptr()) } < 0 {
        return Err(UiFailure::Transport);
    }
    let mode = unsafe { stat.assume_init() }.st_mode & libc::S_IFMT;
    if mode != libc::S_IFIFO && mode != libc::S_IFSOCK {
        return Err(UiFailure::Transport);
    }
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(UiFailure::Transport);
    }
    Ok(())
}
pub struct Pipes {
    state: Arc<Mutex<PipeState>>,
    ending: Arc<AtomicBool>,
    done: Arc<AtomicBool>,
    fault: Arc<AtomicBool>,
}
impl Pipes {
    fn start() -> Result<Self, UiFailure> {
        private_pipe(0)?;
        private_pipe(1)?;
        let state = Arc::new(Mutex::new(PipeState::new()));
        let ending = Arc::new(AtomicBool::new(false));
        let done = Arc::new(AtomicBool::new(false));
        let fault = Arc::new(AtomicBool::new(false));
        let (s, e, d, f) = (state.clone(), ending.clone(), done.clone(), fault.clone());
        thread::Builder::new()
            .name("feedback-pipes".into())
            .spawn(move || {
                let mut completion = WorkerExit::new(d, f.clone());
                let mut read = Fd(0);
                let mut write = Fd(1);
                let mut closing = None;
                loop {
                    if e.load(Ordering::SeqCst) {
                        closing.get_or_insert(Instant::now());
                    }
                    let Ok(mut locked) = s.try_lock() else {
                        thread::sleep(Duration::from_millis(1));
                        continue;
                    };
                    let decision = locked.turn(&mut read, &mut write, closing, Instant::now);
                    if decision.fault {
                        f.store(true, Ordering::SeqCst);
                    }
                    drop(locked);
                    if decision.end {
                        break;
                    }
                    // Nonblocking private descriptors + fixed read/write budgets. No
                    // busy-wait and no mutex held across a blocking poll or sleep.
                    thread::sleep(Duration::from_millis(2));
                }
                completion.mark_clean();
            })
            .map_err(|_| UiFailure::Transport)?;
        Ok(Self {
            state,
            ending,
            done,
            fault,
        })
    }
    fn completion(&self) -> Completion {
        Completion::new(self.state.clone(), self.done.clone(), self.fault.clone())
    }
    fn lock(&self) -> Result<MutexGuard<'_, PipeState>, Diagnostic> {
        let until = Instant::now() + Duration::from_millis(2);
        loop {
            match self.state.try_lock() {
                Ok(state) => return Ok(state),
                Err(TryLockError::Poisoned(_)) => return Err(Diagnostic::TransportLost),
                Err(TryLockError::WouldBlock) if Instant::now() < until => thread::yield_now(),
                Err(_) => return Err(Diagnostic::Contended),
            }
        }
    }
}
impl Transport for Pipes {
    fn failure(&self) -> Option<Diagnostic> {
        if self.fault.load(Ordering::SeqCst) {
            Some(Diagnostic::TransportLost)
        } else {
            None
        }
    }
    fn take_input(&mut self) -> Input {
        if self.fault.load(Ordering::SeqCst) {
            return Input::Failed(Diagnostic::TransportLost);
        }
        // At most 2ms of try-lock retries, then explicit failure; never authorize
        // a Stop against stale facts merely because a worker owns the mailbox.
        match self.lock() {
            Ok(mut s) => s.bridge.take_input(),
            Err(_) => Input::Failed(Diagnostic::Contended),
        }
    }
    fn enqueue(&mut self, message: RendererMessage) -> Result<(), Diagnostic> {
        let mut state = self.lock()?;
        state.enqueue(message, Instant::now)
    }
    fn enqueue_ready(&mut self, ready: Ready) -> Result<ReadyEnqueue, Diagnostic> {
        let mut state = self.lock()?;
        if self.fault.load(Ordering::SeqCst) {
            return Err(Diagnostic::TransportLost);
        }
        // The worker cannot observe EOF between this check and queue insertion:
        // both happen under its same bounded mailbox lock, without X/stdio work.
        state.enqueue_ready(ready, Instant::now)
    }
    fn finish(&mut self) {
        self.ending.store(true, Ordering::SeqCst);
    }
}
impl Drop for Pipes {
    fn drop(&mut self) {
        self.ending.store(true, Ordering::SeqCst);
    }
}

pub fn run(args: &[&str]) -> i32 {
    let guard = match Watchdog::start() {
        Ok(g) => g,
        Err(_) => return 70,
    };
    // SAFETY: handlers have the required C ABI and only set a lock-free atomic;
    // SIGPIPE is ignored so broken output is a transport error, not a surprise abort.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_IGN);
        libc::signal(libc::SIGTERM, signal as *const () as libc::sighandler_t);
        libc::signal(libc::SIGINT, signal as *const () as libc::sighandler_t);
    }
    let result = launch(args, &guard);
    guard.arm(1000);
    match result {
        Ok(code) => code,
        Err(_) => 65,
    }
}
fn launch(args: &[&str], guard: &Watchdog) -> Result<i32, UiFailure> {
    let style = parse_style(args)?;
    if std::env::var_os("DISPLAY").is_none_or(|v| v.is_empty()) {
        return Err(UiFailure::Native);
    }
    if std::env::var_os("WAYLAND_DISPLAY").is_some_and(|s| !s.is_empty())
        || std::env::var("XDG_SESSION_TYPE").is_ok_and(|s| s.eq_ignore_ascii_case("wayland"))
    {
        return Err(UiFailure::Native);
    }
    let pipes = Pipes::start()?;
    let completed = pipes.completion();
    let startup = start_renderer(
        pipes,
        style,
        super::x11::X11Native::connect,
        || guard.origin.elapsed(),
        || {
            let until = Instant::now() + Duration::from_millis(200);
            while !completed.is_done() && Instant::now() < until {
                thread::sleep(Duration::from_millis(2));
            }
        },
    )?;
    let mut runtime = match startup {
        Startup::Running(runtime) => runtime,
        // Startup cancellation still requires the same bounded worker drain
        // receipt. EOF never shortcuts output-failure/completion classification.
        Startup::Closed(exit) => {
            guard.arm(1000);
            return Ok(finish_process(exit, &completed, Instant::now, || {
                thread::sleep(Duration::from_millis(2));
            }));
        }
    };
    guard.arm(2500);
    let mut checked = Instant::now();
    let exit = loop {
        let now = guard.origin.elapsed();
        if TERMINATE.load(Ordering::SeqCst) {
            break Exit::Failure;
        }
        // Always service parent EOF even when native events are queued.
        if let Flow::Exited(exit) = runtime.step(now, Event::Poll) {
            break exit;
        }
        let mut terminal = None;
        for _ in 0..64 {
            let event = match runtime.native_mut().next_event() {
                Ok(Some(event)) => event,
                Ok(None) => break,
                Err(_) => Event::NativeLost,
            };
            if let Flow::Exited(exit) = runtime.step(guard.origin.elapsed(), event) {
                terminal = Some(exit);
                break;
            }
        }
        if let Some(exit) = terminal {
            break exit;
        }
        if checked.elapsed() >= Duration::from_millis(500) {
            let event = match runtime.native_mut().refresh() {
                Ok(true) => Event::TopologyChanged,
                Ok(false) => Event::Poll,
                Err(_) => Event::NativeLost,
            };
            if let Flow::Exited(exit) = runtime.step(guard.origin.elapsed(), event) {
                break exit;
            }
            checked = Instant::now();
        }
        guard.arm(2500); // only completed UI turns extend liveness
        thread::sleep(Duration::from_millis(8));
    };
    guard.arm(1000);
    runtime.shutdown(exit);
    drop(runtime);
    Ok(finish_process(exit, &completed, Instant::now, || {
        thread::sleep(Duration::from_millis(2));
    }))
}
