//! Disposable harness-only standard-FD sentinels. Parent restores its originals
//! at the observed startup gate; EOF then requires the raw child copies closed.
use super::*;
pub(super) struct Restore {
    target: libc::c_int,
    saved: OwnedFd,
}
impl Drop for Restore {
    fn drop(&mut self) {
        unsafe {
            libc::dup2(self.saved.as_raw_fd(), self.target);
        }
    }
}
pub(super) fn stdio() -> (Vec<UnixStream>, Vec<Restore>) {
    let mut peers = Vec::new();
    let mut restore = Vec::new();
    for target in 0..=2 {
        let saved = unsafe { libc::fcntl(target, libc::F_DUPFD_CLOEXEC, 64) };
        assert!(
            saved >= 64,
            "fixture must preserve parent standard descriptor"
        );
        let saved = unsafe { OwnedFd::from_raw_fd(saved) };
        let (peer, inherited) = UnixStream::pair().unwrap();
        peer.set_read_timeout(Some(Duration::from_millis(300)))
            .unwrap();
        assert_eq!(unsafe { libc::dup2(inherited.as_raw_fd(), target) }, target);
        restore.push(Restore { target, saved });
        peers.push(peer);
    }
    (peers, restore)
}
