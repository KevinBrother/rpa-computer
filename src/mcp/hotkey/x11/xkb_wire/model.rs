//! Owned, byte-decoded map values. No generated dynamic-buffer getters.
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Map {
    pub device: u8,
    pub min_key: u8,
    pub max_key: u8,
    pub types: BTreeMap<u8, KeyType>,
    pub symbols: BTreeMap<u8, Symbols>,
    pub actions: BTreeMap<u8, Vec<Action>>,
    pub behaviors: BTreeMap<u8, [u8; 2]>,
    pub virtuals: [u8; 16],
    pub modifiers: [u8; 256],
    pub explicit: BTreeMap<u8, u8>,
    pub virtual_modifiers: BTreeMap<u8, u16>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct KeyType {
    pub mask: u8,
    pub real: u8,
    pub virtuals: u16,
    pub levels: u8,
    pub entries: Vec<TypeEntry>,
    pub preserve: Option<Vec<Mods>>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TypeEntry {
    pub active: bool,
    pub mask: u8,
    pub level: u8,
    pub real: u8,
    pub virtuals: u16,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Mods {
    pub mask: u8,
    pub real: u8,
    pub virtuals: u16,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Symbols {
    pub types: [u8; 4],
    pub group_info: u8,
    pub width: u8,
    pub symbols: Vec<u32>,
}

/// Preserve ALL eight bytes, including unknown/private action types. The wire
/// reader never decides a dependency's semantics and never constructs an enum
/// with a potentially invalid discriminant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Action(pub [u8; 8]);
