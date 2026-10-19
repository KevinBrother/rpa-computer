//! Bounded ICCCM reply planning. No INCR, old clipboard reads or manager promises.
use super::policy::{error, MAX_BYTES};
use crate::backend::BackendError;
use std::collections::HashSet;

pub(super) const MAX_MULTIPLE_PAIRS: usize = 64;
#[derive(Clone, Copy)]
pub(super) struct Atoms {
    pub clipboard: u32,
    pub targets: u32,
    pub utf8: u32,
    pub timestamp: u32,
    pub multiple: u32,
    pub atom: u32,
    pub atom_pair: u32,
    pub integer: u32,
    pub incr: u32,
}
#[derive(Clone, Copy, Debug)]
pub(super) struct Request {
    pub owner: u32,
    pub requestor: u32,
    pub selection: u32,
    pub target: u32,
    pub property: u32,
    pub time: u32,
}
pub(super) struct MultipleProperty {
    pub type_atom: u32,
    pub format: u8,
    pub words: Vec<u32>,
    pub bytes_after: u32,
}
#[derive(Debug, PartialEq, Eq)]
pub(super) enum PropertyData {
    Bytes(Vec<u8>),
    Words(Vec<u32>),
}
pub(super) struct Write {
    pub window: u32,
    pub property: u32,
    pub type_atom: u32,
    pub format: u8,
    pub data: PropertyData,
}
pub(super) struct Reply {
    pub notify: Request,
    pub writes: Vec<Write>,
}
pub(super) struct SelectionData {
    atoms: Atoms,
    owner: u32,
    acquired: u32,
    bytes: Vec<u8>,
    active: bool,
}
/// X server timestamps wrap; a half-range ambiguity is conservatively old.
pub(super) fn before(time: u32, origin: u32) -> bool {
    (time.wrapping_sub(origin) as i32) < 0
}
impl SelectionData {
    pub fn new(
        atoms: Atoms,
        owner: u32,
        acquired: u32,
        bytes: Vec<u8>,
        inline_budget: usize,
    ) -> Result<Self, BackendError> {
        if owner == 0 || acquired == 0 || bytes.len() > MAX_BYTES || bytes.len() > inline_budget {
            return Err(error("invalid bounded clipboard owner/payload/timestamp"));
        }
        Ok(Self {
            atoms,
            owner,
            acquired,
            bytes,
            active: true,
        })
    }
    pub fn ownership_lost(&mut self) {
        self.stop();
    }
    pub fn stop(&mut self) {
        self.active = false;
        self.bytes.clear();
    }
    #[cfg(target_os = "linux")]
    pub fn acquired(&self) -> u32 {
        self.acquired
    }
    pub fn eligible(&self, req: Request) -> bool {
        self.active
            && req.owner == self.owner
            && req.requestor != 0
            && req.selection == self.atoms.clipboard
            && (req.time == 0 || !before(req.time, self.acquired))
    }
    fn conversion(&self, req: Request, target: u32, property: u32) -> Option<Write> {
        let a = self.atoms;
        if property == 0 || target == a.incr {
            return None;
        }
        let (type_atom, format, data) = if target == a.targets {
            (
                a.atom,
                32,
                PropertyData::Words(vec![a.targets, a.utf8, a.timestamp, a.multiple]),
            )
        } else if target == a.utf8 {
            (a.utf8, 8, PropertyData::Bytes(self.bytes.clone()))
        } else if target == a.timestamp {
            (a.integer, 32, PropertyData::Words(vec![self.acquired]))
        } else {
            return None;
        };
        Some(Write {
            window: req.requestor,
            property,
            type_atom,
            format,
            data,
        })
    }
    pub fn reply(&self, req: Request, pairs: Option<MultipleProperty>) -> Reply {
        let mut result = Reply {
            notify: Request { property: 0, ..req },
            writes: vec![],
        };
        if !self.eligible(req) {
            return result;
        }
        if req.target != self.atoms.multiple {
            let property = if req.property == 0 {
                req.target
            } else {
                req.property
            };
            if let Some(write) = self.conversion(req, req.target, property) {
                result.writes.push(write);
                result.notify.property = property;
            }
            return result;
        }
        let Some(mut pairs) = pairs else {
            return result;
        };
        if req.property == 0
            || pairs.type_atom != self.atoms.atom_pair
            || pairs.format != 32
            || pairs.bytes_after != 0
            || pairs.words.len() % 2 != 0
            || pairs.words.len() > MAX_MULTIPLE_PAIRS * 2
        {
            return result;
        }
        let mut destinations = HashSet::new();
        for pair in pairs.words.chunks_exact(2) {
            if pair[1] != 0 && !destinations.insert(pair[1]) {
                return result;
            }
        }
        for pair in pairs.words.chunks_exact_mut(2) {
            if pair[0] == self.atoms.multiple || pair[1] == req.property {
                pair[1] = 0;
            } else if let Some(write) = self.conversion(req, pair[0], pair[1]) {
                result.writes.push(write);
            } else {
                pair[1] = 0;
            }
        }
        result.writes.push(Write {
            window: req.requestor,
            property: req.property,
            type_atom: self.atoms.atom_pair,
            format: 32,
            data: PropertyData::Words(pairs.words),
        });
        result.notify.property = req.property;
        result
    }
}
