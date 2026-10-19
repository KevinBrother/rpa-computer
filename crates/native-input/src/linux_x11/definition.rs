//! Owned XKB single-key value model; no Xlib linkage.
//! Includes per-key repeat ownership; unknown observations cannot prove restoration.
use std::os::raw::c_ulong as KeySym;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Definition {
    pub(super) types: [u8; 4],
    pub(super) group_info: u8,
    pub(super) width: u8,
    pub(super) symbols: Vec<KeySym>,
    pub(super) repeat: Option<bool>,
    pub(super) actions: Vec<[u8; 8]>,
    pub(super) behavior: [u8; 2],
    pub(super) explicit: u8,
    pub(super) modmap: u8,
    pub(super) vmodmap: u16,
}
impl Definition {
    pub fn has_effects(&self) -> bool {
        !self.actions.is_empty() || self.behavior != [0; 2] || self.vmodmap != 0
    }
    pub fn is_modifier(&self) -> bool {
        self.modmap != 0 || self.vmodmap != 0
    }
    pub fn for_symbol(symbol: KeySym) -> Self {
        Self {
            types: [0; 4], // XkbOneLevelIndex, validated on the live descriptor.
            group_info: 1,
            width: 1,
            symbols: vec![symbol],
            repeat: None,
            actions: Vec::new(),
            behavior: [0; 2],
            // Explicit types + interpretation prevent compatibility-map
            // recomputation from attaching actions or inferring case levels.
            explicit: 0x1f,
            modmap: 0,
            vmodmap: 0,
        }
    }
}

impl Definition {
    /// Temporary full-map restoration state. The final request must restore
    /// the original explicit flags separately, without recomputing actions.
    pub fn guarded_restoration(&self) -> Self {
        let mut staged = self.clone();
        staged.explicit |= 0x30;
        staged
    }

    pub fn restoration_matches(&self, observed: &Self) -> bool {
        self.repeat.is_some() && self == observed
    }
}

#[cfg(test)]
#[path = "definition_tests.rs"]
mod definition_tests;

// Portable writer-selection seam; linked by actual xkb writer and pure tests.
#[path = "map_backend.rs"]
pub(crate) mod map_backend;
