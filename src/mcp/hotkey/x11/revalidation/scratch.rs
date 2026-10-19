//! Single announced component/range only. No wholesale rebasing after an
//! unrelated notification, no unknown-action erasure or companion suppression.
use super::{Baseline, Scratch, ScratchPart};
use std::collections::BTreeMap;
use xcb::{x, xkb};

pub(super) fn range(baseline: &Baseline, change: Scratch) -> Result<std::ops::Range<u16>, String> {
    let first = u16::from(change.first);
    let end = first
        .checked_add(u16::from(change.count))
        .filter(|end| *end <= 256)
        .ok_or("XKB scratch range wraps key/type index space")?;
    if first == end {
        return Err("XKB scratch range is empty".into());
    }
    let protected = match change.part {
        ScratchPart::Types => &baseline.validated.snapshot.dependency_types,
        _ => &baseline.validated.snapshot.dependency_keys,
    };
    if protected
        .iter()
        .any(|key| (first..end).contains(&u16::from(*key)))
    {
        return Err("XKB scratch range overlaps emergency dependency".into());
    }
    if matches!(change.part, ScratchPart::Types) {
        if (first..end).any(|index| !baseline.image.map.types.contains_key(&(index as u8))) {
            return Err("XKB scratch type range outside authorized full map".into());
        }
    } else if first < u16::from(baseline.image.map.min_key)
        || end > u16::from(baseline.image.map.max_key) + 1
    {
        return Err("XKB scratch key range outside authorized full map".into());
    }
    Ok(first..end)
}

fn replace<T: Clone>(
    old: &mut BTreeMap<u8, T>,
    new: &BTreeMap<u8, T>,
    range: std::ops::Range<u16>,
) {
    for key in range {
        let key = key as u8; // admitted range, exclusive end<=256
        old.remove(&key);
        if let Some(value) = new.get(&key) {
            old.insert(key, value.clone());
        }
    }
}

pub(super) fn validate_delta(
    old: &Baseline,
    new: &Baseline,
    change: Scratch,
) -> Result<(), String> {
    let range = range(old, change)?;
    range_check_new(new, change)?;
    if old.plan != new.plan
        || old.validated.modifiers != new.validated.modifiers
        || old.validated.snapshot.dependency_keys != new.validated.snapshot.dependency_keys
        || old.validated.snapshot.dependency_types != new.validated.snapshot.dependency_types
    {
        return Err("XKB scratch edit changes grabs/physical identity/state dependencies".into());
    }
    // Start with the authorized image. Replace ONLY the admitted fields/range,
    // then demand exact equality to the entire new image, including all controls,
    // compatibility, modifier/vmod bindings and device/range identity.
    let mut allowed = old.image.clone();
    match change.part {
        ScratchPart::Symbols => {
            if old.image.core.first_keycode != new.image.core.first_keycode
                || old.image.core.keysyms_per_keycode != new.image.core.keysyms_per_keycode
                || old.image.core.keysyms.len() != new.image.core.keysyms.len()
            {
                return Err("XKB scratch symbols changed core layout".into());
            }
            let row = usize::from(change.first)
                .checked_sub(usize::from(old.image.core.first_keycode))
                .ok_or("scratch row before core origin")?;
            let width = old.image.core.keysyms_per_keycode;
            let start = row
                .checked_mul(width)
                .ok_or("scratch symbol offset overflow")?;
            let end = usize::from(change.count)
                .checked_mul(width)
                .and_then(|n| start.checked_add(n))
                .ok_or("scratch symbol extent overflow")?;
            let dst = allowed
                .core
                .keysyms
                .get_mut(start..end)
                .ok_or("scratch core row missing")?;
            let src = new
                .image
                .core
                .keysyms
                .get(start..end)
                .ok_or("scratch new core row missing")?;
            dst.copy_from_slice(src);
            replace(&mut allowed.map.symbols, &new.image.map.symbols, range);
        }
        ScratchPart::Types => replace(&mut allowed.map.types, &new.image.map.types, range),
        ScratchPart::Actions => {
            for key in range.clone() {
                let before = old
                    .image
                    .map
                    .actions
                    .get(&(key as u8))
                    .map(Vec::as_slice)
                    .unwrap_or(&[]);
                let after = new
                    .image
                    .map
                    .actions
                    .get(&(key as u8))
                    .map(Vec::as_slice)
                    .unwrap_or(&[]);
                if before != after
                    && (before.len() != after.len()
                        || before.iter().chain(after).any(|a| a.0[0] != 0))
                {
                    return Err("scratch action delta is not inert NoAction data".into());
                }
            }
            replace(&mut allowed.map.actions, &new.image.map.actions, range);
        }
        ScratchPart::Behaviors => {
            for key in range.clone() {
                if [old, new].iter().any(|image| {
                    image
                        .image
                        .map
                        .behaviors
                        .get(&(key as u8))
                        .is_some_and(|b| b[0] != 0)
                }) {
                    return Err("scratch behavior is not inert default behavior".into());
                }
            }
            replace(&mut allowed.map.behaviors, &new.image.map.behaviors, range);
        }
        ScratchPart::Explicit => replace(&mut allowed.map.explicit, &new.image.map.explicit, range),
    }
    if allowed != new.image {
        return Err("XKB scratch delta exceeds announced safe fields/range".into());
    }
    Ok(())
}

fn range_check_new(new: &Baseline, change: Scratch) -> Result<(), String> {
    // Also reject a changed map whose new dependency set/range conflicts with
    // the declared update before any cache assignment can occur.
    range(new, change).map(|_| ())
}

pub(super) fn notice(event: &xcb::Event) -> Result<Option<Scratch>, String> {
    let result = match event {
        xcb::Event::X(x::Event::MappingNotify(e)) if e.response_type() & 0x80 == 0 => {
            match e.request() {
                x::Mapping::Keyboard => Some(Scratch {
                    part: ScratchPart::Symbols,
                    first: e.first_keycode(),
                    count: e.count(),
                }),
                x::Mapping::Pointer => None,
                _ => return Err("core modifier notification is not a scratch update".into()),
            }
        }
        xcb::Event::Xkb(xkb::Event::MapNotify(e)) if e.response_type() & 0x80 == 0 => {
            let bits = e.changed().bits();
            // Do not guess per-component ranges from a combined/unknown mask.
            let (part, first, count) = if bits == xkb::MapPart::KEY_SYMS.bits() {
                (ScratchPart::Symbols, e.first_key_sym(), e.n_key_syms())
            } else if bits == xkb::MapPart::KEY_TYPES.bits() {
                (ScratchPart::Types, e.first_type(), e.n_types())
            } else if bits == xkb::MapPart::KEY_ACTIONS.bits() {
                (ScratchPart::Actions, e.first_key_act(), e.n_key_acts())
            } else if bits == xkb::MapPart::KEY_BEHAVIORS.bits() {
                (
                    ScratchPart::Behaviors,
                    e.first_key_behavior(),
                    e.n_key_behavior(),
                )
            } else if bits == xkb::MapPart::EXPLICIT_COMPONENTS.bits() {
                (
                    ScratchPart::Explicit,
                    e.first_key_explicit(),
                    e.n_key_explicit(),
                )
            } else {
                return Err("combined/structural/unknown XKB scratch notification".into());
            };
            Some(Scratch { part, first, count })
        }
        _ => None,
    };
    Ok(result)
}
