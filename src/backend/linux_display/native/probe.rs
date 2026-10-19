//! Test-build-only observation points in the REAL GetImage/decode path.
//! No frames, topology, permits, requests or errors are substituted.
use rpa_display_topology::NativeRect;
use std::{cell::Cell, rc::Rc};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Reads {
    pub requests: u64,
    pub replies: u64,
    pub reply_bytes: u64,
    pub decode_entries: u64,
    pub decoded_rgba_bytes: u64,
    pub last_bounds: Option<NativeRect>,
    pub last_permit_bytes: u64,
}
#[derive(Clone, Default)]
pub(crate) struct CaptureProbe(Rc<Cell<Reads>>);
impl CaptureProbe {
    pub fn read(&self) -> Reads {
        self.0.get()
    }
    pub(super) fn request(&self, bounds: NativeRect, permit_bytes: u64) {
        let mut reads = self.read();
        reads.requests += 1;
        reads.last_bounds = Some(bounds);
        reads.last_permit_bytes = permit_bytes;
        self.0.set(reads);
    }
    pub(super) fn reply(&self, bytes: usize) {
        let mut reads = self.read();
        reads.replies += 1;
        reads.reply_bytes += bytes as u64;
        self.0.set(reads);
    }
    pub(super) fn decode_entry(&self) {
        let mut reads = self.read();
        reads.decode_entries += 1;
        self.0.set(reads);
    }
    pub(super) fn decoded(&self, bytes: usize) {
        let mut reads = self.read();
        reads.decoded_rgba_bytes += bytes as u64;
        self.0.set(reads);
    }
}
