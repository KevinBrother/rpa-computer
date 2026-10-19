//! Scripted observations, not a replacement equality/validation algorithm.
use super::*;
use crate::mcp::hotkey::x11::semantic_image::SemanticImage;
use crate::mcp::hotkey::x11::xkb_aux_wire::{Compatibility, Controls};
use crate::mcp::hotkey::x11::xkb_wire::{Action, KeyType, Map, Symbols};
use crate::mcp::hotkey::x11_plan::KeyboardMap;
use std::collections::{BTreeMap, VecDeque};

pub fn image() -> SemanticImage {
    let mut slots: [Vec<u8>; 8] = Default::default();
    for (slot, key) in [(0, 11), (1, 13), (2, 10), (3, 12)] {
        slots[slot] = vec![key];
    }
    let core = KeyboardMap {
        first_keycode: 9,
        keysyms_per_keycode: 1,
        keysyms: vec![0xff1b, 0xffe3, 0xffe1, 0xffe9, 0xffe5, 0x61],
        modifier_keycodes: slots,
    };
    let mut map = Map {
        device: 3,
        min_key: 9,
        max_key: 14,
        types: BTreeMap::new(),
        symbols: BTreeMap::new(),
        actions: BTreeMap::new(),
        behaviors: BTreeMap::new(),
        virtuals: [0; 16],
        modifiers: [0; 256],
        explicit: BTreeMap::new(),
        virtual_modifiers: BTreeMap::new(),
    };
    for index in [0, 1] {
        map.types.insert(
            index,
            KeyType {
                mask: 0,
                real: 0,
                virtuals: 0,
                levels: 1,
                entries: vec![],
                preserve: None,
            },
        );
    }
    for (index, &sym) in core.keysyms.iter().enumerate() {
        let key = 9 + index as u8;
        map.symbols.insert(
            key,
            Symbols {
                types: [u8::from(key == 14), 0, 0, 0],
                group_info: 1,
                width: 1,
                symbols: vec![sym],
            },
        );
        map.actions.insert(key, vec![Action([0; 8])]);
    }
    for (key, kind, mask) in [(10, 1, 4), (11, 1, 1), (12, 1, 8), (13, 3, 2)] {
        map.modifiers[key as usize] = mask;
        map.actions
            .insert(key, vec![Action([kind, 0, mask, mask, 0, 0, 0, 0])]);
    }
    let mut controls = Controls {
        device: 3,
        fields: [0; 81],
    };
    controls.fields[1] = 1; // numGroups, not current locked/held/group state
    SemanticImage {
        core,
        map,
        controls,
        compatibility: Compatibility {
            device: 3,
            interpretations: vec![],
            groups: [[0; 4]; 4],
        },
    }
}

pub fn notice() -> Notice {
    Notice {
        device: 3,
        old_device: 3,
        min: 9,
        max: 14,
        old_min: 9,
        old_max: 14,
        changed: 3,
        synthetic: false,
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Receipt {
    Press(u8),
    Release(u8),
    Other(u8),
    Fatal(Invalidation),
}

pub struct Script {
    pub images: VecDeque<Result<SemanticImage, String>>,
    pub events: VecDeque<Receipt>,
    pub trace: Vec<&'static str>,
    pub deadlines: Vec<Instant>,
    pub generation: usize,
    pub now: Instant,
    pub stopped: bool,
    pub failed: bool,
    pub expire_at_fence: bool,
    pub stop_at_fence: bool,
    pub fail_at_fence: bool,
    pub replace_at_fence: bool,
    pub fence_error: bool,
    pub queue_error: bool,
}
impl Script {
    pub fn new(a: SemanticImage, b: SemanticImage) -> Self {
        Self {
            images: VecDeque::from([Ok(a), Ok(b)]),
            events: VecDeque::new(),
            trace: vec![],
            deadlines: vec![],
            generation: 7,
            now: Instant::now(),
            stopped: false,
            failed: false,
            expire_at_fence: false,
            stop_at_fence: false,
            fail_at_fence: false,
            replace_at_fence: false,
            fence_error: false,
            queue_error: false,
        }
    }
    pub fn good() -> Self {
        Self::new(image(), image())
    }
}
impl Source for Script {
    type Event = Receipt;
    fn generation(&self) -> usize {
        self.generation
    }
    fn checkpoint(&mut self, deadline: Instant) -> Result<(), String> {
        self.deadlines.push(deadline);
        if self.stopped || self.failed || self.now >= deadline {
            Err("script lifecycle/expiry".into())
        } else {
            Ok(())
        }
    }
    fn read_image(&mut self, deadline: Instant) -> Result<SemanticImage, String> {
        self.checkpoint(deadline)?;
        self.trace.push("image");
        self.images
            .pop_front()
            .expect("no retries or third observation")
    }
    fn fence(&mut self, deadline: Instant) -> Result<(), String> {
        self.checkpoint(deadline)?;
        self.trace.push("fence");
        if self.expire_at_fence {
            self.now = deadline;
        }
        if self.stop_at_fence {
            self.stopped = true;
        }
        if self.fail_at_fence {
            self.failed = true;
        }
        if self.replace_at_fence {
            self.generation += 1;
        }
        if self.fence_error {
            Err("script fence disconnected".into())
        } else {
            Ok(())
        }
    }
    fn invalidation(&self, event: &Receipt) -> Option<Invalidation> {
        match event {
            Receipt::Fatal(reason) => Some(*reason),
            _ => None,
        }
    }
    fn queued(&mut self, deadline: Instant) -> Result<Option<Receipt>, String> {
        self.checkpoint(deadline)?;
        self.trace.push("queued");
        if self.queue_error {
            Err("script queue disconnected".into())
        } else {
            Ok(self.events.pop_front())
        }
    }
}

pub fn deadline() -> Instant {
    Instant::now() + std::time::Duration::from_millis(200)
}
pub fn baseline() -> Baseline {
    // Uses actual planner + validate_owned, never a fixture-injected safe flag.
    Baseline::validated(7, image()).expect("real semantic fixture is safe")
}
pub fn positive_control() {
    let mut baseline = baseline();
    let mut source = Script::good();
    let mut pending = VecDeque::new();
    revalidate(
        &mut baseline,
        Trigger::Equivalent(notice()),
        &mut source,
        &mut pending,
        deadline(),
    )
    .expect("RED positive control: complete equivalent NKN must be proved, not ignored");
}
