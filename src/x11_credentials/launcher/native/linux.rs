//! Parent-side native adapter. Only the assembly-selected leaf is child code.
mod deadline;
mod leaf;
mod operation;
mod process;
#[cfg(test)]
mod publication_tests;
mod raw;
mod transport;
use super::super::policy::{Adapter, Checkpoint, Errno, Method, Observation, SignalResult, Spawn};
#[cfg(test)]
use super::lifecycle::{ParentPoint, Probe as LifecycleProbe};
use crate::x11_credentials::{ipc::AuthorityRead, FailureKind, Request, MAX_AUTH_BYTES};
use operation::{Operation, Resources};
pub(in crate::x11_credentials::launcher) use process::Handle;
use std::ffi::CString;
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
#[cfg(test)]
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

/// Kernel-sized mask includes signals glibc's pthread_sigmask intentionally
/// leaves unblocked. Restored in parent on every return, and as an unwind guard.
pub(in crate::x11_credentials::launcher) struct Mask(Option<u64>);
impl Mask {
    fn restore(&mut self) -> Result<(), FailureKind> {
        let Some(old) = self.0 else {
            return Ok(());
        };
        let result = unsafe {
            raw::syscall(
                libc::SYS_rt_sigprocmask,
                [
                    libc::SIG_SETMASK as usize,
                    &old as *const u64 as usize,
                    0,
                    8,
                    0,
                    0,
                ],
            )
        };
        if result != 0 {
            return Err(FailureKind::ChildSetupFailed);
        }
        self.0 = None;
        Ok(())
    }
}
impl Drop for Mask {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}
pub(in crate::x11_credentials::launcher) struct Native<'a> {
    operation: Option<Arc<Operation>>,
    handle: Option<Handle>,
    validated: bool,
    signal_attempted: bool,
    deadline: Option<Instant>,
    method: Method,
    clone3_errno: Option<i32>,
    legacy_only: bool,
    pause: u8,
    #[cfg(test)]
    observer: Option<&'a mut dyn FnMut(super::fixture::Event<'_>)>,
    #[cfg(test)]
    lifecycle: Option<LifecycleProbe>,
    lifetime: std::marker::PhantomData<&'a ()>,
    // Preserve original-creator-only resource/GO access after moving storage
    // into a shared record. Reaper handles are Send, the Native executor is not.
    not_send: std::marker::PhantomData<*mut ()>,
}
impl<'a> Native<'a> {
    pub fn new() -> Self {
        Self {
            operation: None,
            handle: None,
            validated: false,
            signal_attempted: false,
            deadline: None,
            method: Method::Clone3,
            clone3_errno: None,
            legacy_only: false,
            pause: 0,
            #[cfg(test)]
            observer: None,
            #[cfg(test)]
            lifecycle: None,
            lifetime: std::marker::PhantomData,
            not_send: std::marker::PhantomData,
        }
    }
    #[cfg(test)]
    pub(in crate::x11_credentials::launcher) fn with_probe(
        probe: &'a mut super::fixture::Probe<'_>,
    ) -> Self {
        let mut native = Self::new();
        native.legacy_only = matches!(probe.mode, super::fixture::Mode::LegacyOnly);
        native.pause = probe.pause.map(stage_number).unwrap_or(0);
        native.observer = Some(&mut *probe.parent_observer);
        native
    }
    #[cfg(test)]
    pub(in crate::x11_credentials::launcher) fn with_lifecycle(probe: LifecycleProbe) -> Self {
        let mut native = Self::new();
        native.lifecycle = Some(probe);
        native
    }
    fn retire_resources(&mut self) {
        self.revoke_ipc();
        // Preserve the existing panic-safety signal request, without waiting.
        if self.validated && !self.signal_attempted {
            if let Some(handle) = self.handle.as_ref() {
                if handle.observe() != Observation::Reaped {
                    let _ = handle.kill();
                }
            }
        }
        self.handle.take();
        if let Some(operation) = self.operation.take() {
            // Only this original Native can access resources. All transport/
            // callback borrows ended before the outer foreground guard retires.
            unsafe {
                operation.retire_resources();
            }
        }
        self.validated = false;
        self.signal_attempted = false;
        self.deadline = None;
    }
    fn deadline(&self) -> Result<Instant, FailureKind> {
        self.deadline.ok_or(FailureKind::ChildSetupFailed)
    }
    fn marker(&mut self, stage: u8) -> Result<(), FailureKind> {
        let deadline = self.deadline()?;
        let operation = self
            .operation
            .as_ref()
            .ok_or(FailureKind::ChildSetupFailed)?;
        if operation.cancelled() {
            return Err(FailureKind::Timeout);
        }
        let resources =
            unsafe { operation.resources_mut() }.ok_or(FailureKind::ChildSetupFailed)?;
        let socket = resources
            .parent
            .as_mut()
            .ok_or(FailureKind::ChildSetupFailed)?;
        transport::byte(socket, stage, deadline)?;
        #[cfg(test)]
        if let Some(observer) = self.observer.as_mut() {
            let handle = self.handle.as_ref().ok_or(FailureKind::ChildSetupFailed)?;
            let stage = match stage {
                1 => super::fixture::Stage::StartupGate,
                2 => super::fixture::Stage::BeforeRead,
                _ => super::fixture::Stage::BeforeFrame,
            };
            observer(super::fixture::Event {
                stage,
                pidfd: handle.fd().ok_or(FailureKind::ChildSetupFailed)?,
                method: self.method,
                clone3_errno: self.clone3_errno,
            });
        }
        transport::live(deadline)?;
        if self.pause == stage {
            // Test leaf is actually waiting on private IPC. Hold permission until
            // deadline, not an artificial adapter Timeout before the child exists.
            let mut byte = [0];
            let _ = transport::read(socket, &mut byte, deadline)?;
            return Err(FailureKind::Protocol);
        }
        transport::allow(socket, deadline, |point| {
            #[cfg(test)]
            if let Some(probe) = self.lifecycle.as_ref() {
                probe.parent(match point {
                    transport::WritePoint::Before => ParentPoint::BeforePermissionWrite,
                    transport::WritePoint::After => ParentPoint::AfterPermissionWrite,
                });
            }
            #[cfg(not(test))]
            let _ = point;
        })
    }
    fn spawn(&mut self, method: Method) -> Spawn<Handle> {
        let Some(operation) = self.operation.as_ref() else {
            return Spawn::Unavailable;
        };
        let index = match operation.enter(method) {
            Ok(index) => index,
            Err(_) => {
                return if operation.finish_no_child() {
                    Spawn::Unavailable
                } else {
                    Spawn::Uncertain
                }
            }
        };
        let process = &operation.attempts[index];
        self.method = method;
        // Resources are pre-owned by the tracker; only this !Send Native has
        // access. No unique reference to the concurrently published Process.
        let Some(resources) = (unsafe { operation.resources_mut() }) else {
            return Spawn::Uncertain;
        };
        let child = &mut *resources.child as *mut leaf::Child;
        let result = unsafe {
            if method == Method::Clone3 {
                let args = raw::CloneArgs {
                    flags: libc::CLONE_PIDFD as u64,
                    pidfd: process.output_pointer() as u64,
                    exit_signal: libc::SIGCHLD as u64,
                    ..Default::default()
                };
                raw::x11_credential_clone3(
                    &args,
                    std::mem::size_of::<raw::CloneArgs>(),
                    child,
                    process.publication_pointer(),
                )
            } else {
                raw::x11_credential_legacy(
                    (libc::CLONE_PIDFD | libc::SIGCHLD) as usize,
                    process.output_pointer(),
                    child,
                    process.publication_pointer(),
                )
            }
        };
        // Raw parent has release-published before reaching any Rust or hook.
        // Never reinterpret a mismatched/unpublished record as a usable result.
        if process.result() != result
            || (result <= 0 && !process.confirmed_error())
            || result > i32::MAX as isize
        {
            return Spawn::Uncertain;
        }
        let handle = operation.handles()[index].clone();
        if result < 0 {
            let errno = (-result) as i32;
            if method == Method::Clone3 {
                self.clone3_errno = Some(errno);
            }
            return Spawn::NoChild(match errno {
                libc::ENOSYS => Errno::Enosys,
                libc::EPERM => Errno::Eperm,
                libc::EACCES => Errno::Eacces,
                libc::EAGAIN => Errno::Eagain,
                libc::ENOMEM => Errno::Enomem,
                libc::EINVAL => Errno::Einval,
                other => Errno::Other(other),
            });
        }
        // The assembly child path never returns here. Successful kernel return
        // is the ONLY source of pid/pidfd; no pidfd_open or guessed identity.
        #[cfg(test)]
        if let Some(probe) = self.lifecycle.as_ref() {
            probe
                .raw()
                .child_pid
                .store(result as i32, Ordering::Release);
            probe.raw().kernel_pidfd.store(
                handle.fd().map(|fd| fd.as_raw_fd()).unwrap_or(-1),
                Ordering::Release,
            );
            probe.parent(ParentPoint::CloneReturned);
        }
        resources.child_endpoint.take();
        let handle = Some(handle);
        self.handle = handle.clone();
        Spawn::Parent {
            pid: result as i32,
            pidfd: handle,
        }
    }
}
#[cfg(test)]
fn stage_number(stage: super::fixture::Stage) -> u8 {
    match stage {
        super::fixture::Stage::StartupGate => 1,
        super::fixture::Stage::BeforeRead => 2,
        super::fixture::Stage::BeforeFrame => 3,
    }
}
impl Adapter for Native<'_> {
    type Handle = Handle;
    type Mask = Mask;
    fn retire_foreground(&mut self) {
        self.retire_resources();
    }
    fn launch_handles(&self) -> Option<[Handle; 2]> {
        self.operation.as_ref().map(Operation::handles)
    }
    fn launch_registered(&mut self) {
        if let Some(operation) = self.operation.as_ref() {
            operation.registered();
        }
    }
    fn owned_attempt(&self) -> Option<usize> {
        Some(if self.method == Method::Clone3 { 0 } else { 1 })
    }
    fn prepare(&mut self, request: &Request<'_>) -> Result<(), FailureKind> {
        let path = request.authority_path.ok_or(FailureKind::InvalidRequest)?;
        let path = CString::new(path.as_os_str().as_encoded_bytes())
            .map_err(|_| FailureKind::InvalidRequest)?;
        let mut bytes = vec![0u8; MAX_AUTH_BYTES + 1];
        let (parent, child_endpoint) =
            UnixStream::pair().map_err(|_| FailureKind::ChildSetupFailed)?;
        parent
            .set_nonblocking(true)
            .map_err(|_| FailureKind::ChildSetupFailed)?;
        let child = Box::new(leaf::Child {
            deadline: deadline::prepare(self.deadline()?)?,
            endpoint: child_endpoint.as_raw_fd(),
            parent_pid: unsafe { raw::syscall(libc::SYS_getpid, [0; 6]) } as i32,
            parent_tid: unsafe { raw::syscall(libc::SYS_gettid, [0; 6]) } as i32,
            path: path.as_ptr().cast(),
            bytes: bytes.as_mut_ptr(),
            stat: unsafe { std::mem::zeroed() },
            frame: [b'X', b'A', b'C', b'R', 1, 0, 0, 0, 0, 0, 0, 0],
            lifecycle: {
                #[cfg(test)]
                {
                    self.lifecycle
                        .as_ref()
                        .map(LifecycleProbe::pointer)
                        .unwrap_or(std::ptr::null_mut())
                }
                #[cfg(not(test))]
                {
                    std::ptr::null_mut()
                }
            },
            checkpoints: {
                #[cfg(test)]
                {
                    self.observer.is_some()
                }
                #[cfg(not(test))]
                {
                    false
                }
            },
        });
        #[cfg(test)]
        if let Some(probe) = self.lifecycle.as_ref() {
            probe
                .raw()
                .creator_tid
                .store(child.parent_tid, Ordering::Release);
            probe
                .raw()
                .parent_socket
                .store(parent.as_raw_fd(), Ordering::Release);
            probe
                .raw()
                .child_socket
                .store(child_endpoint.as_raw_fd(), Ordering::Release);
        }
        self.operation = Some(Operation::new(
            Resources {
                path,
                bytes,
                child,
                parent: Some(parent),
                child_endpoint: Some(child_endpoint),
            },
            self.legacy_only,
            #[cfg(test)]
            self.lifecycle.clone(),
        ));
        Ok(())
    }
    fn checkpoint(&mut self, _: Checkpoint, deadline: Instant) -> Result<(), FailureKind> {
        self.deadline = Some(deadline);
        transport::live(deadline)
    }
    fn block_signals(&mut self) -> Result<Mask, FailureKind> {
        let full = u64::MAX;
        let mut old = 0u64;
        let result = unsafe {
            raw::syscall(
                libc::SYS_rt_sigprocmask,
                [
                    libc::SIG_SETMASK as usize,
                    &full as *const u64 as usize,
                    &mut old as *mut u64 as usize,
                    8,
                    0,
                    0,
                ],
            )
        };
        if result != 0 {
            return Err(FailureKind::ChildSetupFailed);
        }
        Ok(Mask(Some(old)))
    }
    fn restore_signals(&mut self, mut mask: Mask) -> Result<(), FailureKind> {
        if let Some(operation) = self.operation.as_ref() {
            operation.finish_no_child();
        }
        let result = mask.restore();
        #[cfg(test)]
        if self.handle.is_some() {
            if let Some(probe) = self.lifecycle.as_ref() {
                probe.parent(ParentPoint::MaskRestored);
            }
        }
        result
    }
    #[cfg(test)]
    fn lifecycle_attached(&mut self) {
        if let Some(probe) = self.lifecycle.as_ref() {
            probe.parent(ParentPoint::Attached);
        }
    }
    #[cfg(test)]
    fn lifecycle_reply_received(&mut self) {
        if let Some(probe) = self.lifecycle.as_ref() {
            probe.parent(ParentPoint::ReplyReceived);
        }
    }
    fn clone3(&mut self) -> Spawn<Handle> {
        if self.legacy_only {
            self.spawn(Method::Legacy)
        } else {
            self.spawn(Method::Clone3)
        }
    }
    fn legacy_clone_pidfd(&mut self) -> Spawn<Handle> {
        // A direct-legacy fixture is one attempt, not permission for a second.
        if self.legacy_only {
            Spawn::Unavailable
        } else {
            self.spawn(Method::Legacy)
        }
    }
    fn validate_pidfd(&mut self, handle: &Handle) -> Result<(), FailureKind> {
        if handle.valid() {
            self.validated = true;
            Ok(())
        } else {
            Err(FailureKind::ChildSetupFailed)
        }
    }
    fn open_startup_gate(&mut self) -> Result<(), FailureKind> {
        self.marker(1)
    }
    fn revoke_ipc(&mut self) {
        if let Some(operation) = self.operation.as_ref() {
            operation.cancel();
            if let Some(resources) = unsafe { operation.resources_mut() } {
                if let Some(socket) = resources.parent.take() {
                    let _ = socket.shutdown(std::net::Shutdown::Both);
                }
                resources.child_endpoint.take();
            }
        }
    }
    fn receive(&mut self, deadline: Instant) -> Result<AuthorityRead, FailureKind> {
        #[cfg(test)]
        if self.observer.is_some() {
            self.marker(2)?;
            self.marker(3)?;
        }
        let operation = self
            .operation
            .as_ref()
            .ok_or(FailureKind::ChildSetupFailed)?;
        let resources =
            unsafe { operation.resources_mut() }.ok_or(FailureKind::ChildSetupFailed)?;
        transport::receive(
            resources
                .parent
                .as_mut()
                .ok_or(FailureKind::ChildSetupFailed)?,
            deadline,
        )
    }
    fn kill_pidfd(&mut self, handle: &Handle) -> SignalResult {
        self.signal_attempted = true;
        handle.kill()
    }
    fn observe(&mut self, handle: &Handle) -> Observation {
        handle.observe()
    }
}
impl Drop for Native<'_> {
    fn drop(&mut self) {
        // Idempotent after the policy's outer foreground retirement guard.
        self.retire_resources();
    }
}
