//! Action parsing and validation.
//!
//! Every action is fully parsed and validated *before* any input event is
//! produced. Parsing yields a typed [`Action`]; validation against the active
//! observation (coordinate bounds etc.) happens in the session layer. A
//! validated action compiles to a [`Plan`] — a list of key/button names and a
//! sequence of [`PlannedEvent`]s in image coordinates. Translation to native
//! backend coordinates happens once, at the execution boundary.

use crate::runtime::error::{codes, ToolError};

pub const TEXT_MAX_CHARS: usize = 4096;
pub const DRAG_MAX_DURATION_MS: u64 = 5000;
pub const DRAG_DEFAULT_DURATION_MS: u64 = 300;
pub const DRAG_MIN_POINTS: usize = 2;
pub const DRAG_MAX_POINTS: usize = 256;
pub const HOLD_MAX_DURATION_MS: u64 = 5000;
pub const SCROLL_MAX_TICKS: i32 = 100;
pub const CLICK_MAX_COUNT: u8 = 3;

/// Interval between intermediate drag moves.
pub const DRAG_MOVE_INTERVAL_MS: u64 = 8;
/// Pause between repeats of a multi-click.
pub const MULTI_CLICK_INTERVAL_MS: u64 = 90;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

impl MouseButton {
    pub fn name(self) -> &'static str {
        match self {
            MouseButton::Left => "left",
            MouseButton::Right => "right",
            MouseButton::Middle => "middle",
        }
    }
}

/// A fully parsed action. Coordinates are pixels of the image the model saw
/// (top-left origin), *not* native backend units.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Click {
        position: [i32; 2],
        button: MouseButton,
        count: u8,
    },
    Move {
        position: [i32; 2],
    },
    Drag {
        path: Vec<[i32; 2]>,
        button: MouseButton,
        duration_ms: u64,
    },
    Scroll {
        position: [i32; 2],
        delta_x: i32,
        delta_y: i32,
    },
    TextInput {
        text: String,
    },
    KeyChord {
        modifiers: Vec<String>,
        key: String,
    },
    KeyHold {
        key: String,
        duration_ms: u64,
    },
}

impl Action {
    /// All key/button names this action may press. Used to prevalidate names
    /// with the backend before any event is injected.
    pub fn names(&self) -> (Vec<String>, Vec<String>) {
        let mut keys = Vec::new();
        let mut buttons = Vec::new();
        match self {
            Action::Click { button, .. } | Action::Drag { button, .. } => {
                buttons.push(button.name().to_string())
            }
            Action::KeyChord { modifiers, key } => {
                keys.extend(modifiers.iter().cloned());
                keys.push(key.clone());
            }
            Action::KeyHold { key, .. } => keys.push(key.clone()),
            _ => {}
        }
        (keys, buttons)
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Action::Click { .. } => "click",
            Action::Move { .. } => "move",
            Action::Drag { .. } => "drag",
            Action::Scroll { .. } => "scroll",
            Action::TextInput { .. } => "text_input",
            Action::KeyChord { .. } => "key_chord",
            Action::KeyHold { .. } => "key_hold",
        }
    }

    /// Every position referenced by the action (for image-bounds validation).
    pub fn positions(&self) -> Vec<[i32; 2]> {
        match self {
            Action::Click { position, .. }
            | Action::Move { position }
            | Action::Scroll { position, .. } => vec![*position],
            Action::Drag { path, .. } => path.clone(),
            _ => Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Parse an action from its JSON representation. Pure validation only — no
/// coordinate bounds (those require the observation image size) and no
/// backend name checks (those require the backend).
pub fn parse_action(value: &serde_json::Value) -> Result<Action, ToolError> {
    let invalid = |msg: String| ToolError::new(codes::INVALID_ACTION, msg);
    let obj = value
        .as_object()
        .ok_or_else(|| invalid("action must be an object".into()))?;
    let kind = obj
        .get("kind")
        .and_then(|k| k.as_str())
        .ok_or_else(|| invalid("action.kind must be a string".into()))?;

    // Reject unknown fields to catch model typos early instead of silently
    // ignoring a mistyped parameter.
    let known: &[&str] = match kind {
        "click" => &["kind", "position", "button", "count"],
        "move" => &["kind", "position"],
        "drag" => &["kind", "path", "button", "duration_ms"],
        "scroll" => &["kind", "position", "delta_x", "delta_y", "unit"],
        "text_input" => &["kind", "text"],
        "key_chord" => &["kind", "modifiers", "key"],
        "key_hold" => &["kind", "key", "duration_ms"],
        other => {
            return Err(ToolError::new(
                codes::UNSUPPORTED_ACTION,
                format!("unsupported action kind: {other}"),
            ))
        }
    };
    for key in obj.keys() {
        if !known.contains(&key.as_str()) {
            return Err(invalid(format!("action kind {kind} has no field {key}")));
        }
    }

    match kind {
        "click" => {
            let position = parse_position(obj.get("position"))?;
            let button = parse_button(obj.get("button"))?;
            let count = match obj.get("count") {
                None => 1,
                Some(v) => {
                    let n = v
                        .as_u64()
                        .ok_or_else(|| invalid("count must be an integer".into()))?;
                    if !(1..=CLICK_MAX_COUNT as u64).contains(&n) {
                        return Err(invalid(format!(
                            "count must be 1..={CLICK_MAX_COUNT}, got {n}"
                        )));
                    }
                    n as u8
                }
            };
            Ok(Action::Click {
                position,
                button,
                count,
            })
        }
        "move" => Ok(Action::Move {
            position: parse_position(obj.get("position"))?,
        }),
        "drag" => {
            let path = match obj.get("path").and_then(|p| p.as_array()) {
                Some(points) => {
                    let mut path = Vec::with_capacity(points.len());
                    for p in points {
                        path.push(parse_position(Some(p))?);
                    }
                    path
                }
                None => return Err(invalid("drag.path must be an array".into())),
            };
            if !(DRAG_MIN_POINTS..=DRAG_MAX_POINTS).contains(&path.len()) {
                return Err(invalid(format!(
                    "drag.path must have {DRAG_MIN_POINTS}..={DRAG_MAX_POINTS} points, got {}",
                    path.len()
                )));
            }
            let button = parse_button(obj.get("button"))?;
            let duration_ms = parse_duration(
                obj.get("duration_ms"),
                DRAG_DEFAULT_DURATION_MS,
                DRAG_MAX_DURATION_MS,
                "duration_ms",
            )?;
            Ok(Action::Drag {
                path,
                button,
                duration_ms,
            })
        }
        "scroll" => {
            let position = parse_position(obj.get("position"))?;
            match obj.get("unit") {
                None => return Err(invalid("scroll.unit is required".into())),
                Some(v) if v.as_str() == Some("wheel_ticks") => {}
                Some(_) => {
                    return Err(ToolError::new(
                        codes::UNSUPPORTED_ACTION,
                        "scroll unit must be \"wheel_ticks\"; other units are not supported"
                            .to_string(),
                    ))
                }
            }
            let delta_x = parse_i32(obj.get("delta_x"), "delta_x")?;
            let delta_y = parse_i32(obj.get("delta_y"), "delta_y")?;
            if delta_x == 0 && delta_y == 0 {
                return Err(invalid("scroll deltas must not both be zero".into()));
            }
            for (name, d) in [("delta_x", delta_x), ("delta_y", delta_y)] {
                if d.abs() > SCROLL_MAX_TICKS {
                    return Err(invalid(format!(
                        "{name} must be within ±{SCROLL_MAX_TICKS} wheel ticks, got {d}"
                    )));
                }
            }
            Ok(Action::Scroll {
                position,
                delta_x,
                delta_y,
            })
        }
        "text_input" => {
            let text = obj
                .get("text")
                .and_then(|t| t.as_str())
                .ok_or_else(|| invalid("text_input.text must be a string".into()))?;
            let chars = text.chars().count();
            if chars > TEXT_MAX_CHARS {
                return Err(invalid(format!(
                    "text_input.text must be at most {TEXT_MAX_CHARS} characters, got {chars}"
                )));
            }
            Ok(Action::TextInput {
                text: text.to_string(),
            })
        }
        "key_chord" => {
            let modifiers = match obj.get("modifiers").and_then(|m| m.as_array()) {
                Some(list) => {
                    let mut out = Vec::with_capacity(list.len());
                    for m in list {
                        let name = parse_key_name(m.as_str())?;
                        out.push(name);
                    }
                    out
                }
                None => return Err(invalid("key_chord.modifiers must be an array".into())),
            };
            let key = parse_key_name(obj.get("key").and_then(|k| k.as_str()))?;
            if modifiers.is_empty() {
                return Err(invalid(
                    "key_chord.modifiers must contain at least one modifier".into(),
                ));
            }
            for m in &modifiers {
                if !is_modifier(m) {
                    return Err(invalid(format!("{m} is not a modifier key")));
                }
            }
            let mut dedup = modifiers.clone();
            dedup.sort();
            dedup.dedup();
            if dedup.len() != modifiers.len() {
                return Err(invalid("duplicate modifier in key_chord".into()));
            }
            if modifiers.iter().any(|m| m == &key) {
                return Err(invalid(
                    "key_chord key must differ from its modifiers".into(),
                ));
            }
            if is_modifier(&key) {
                return Err(invalid("key_chord key must not be a modifier key".into()));
            }
            Ok(Action::KeyChord { modifiers, key })
        }
        "key_hold" => {
            let key = parse_key_name(obj.get("key").and_then(|k| k.as_str()))?;
            let duration_ms = parse_duration(
                obj.get("duration_ms"),
                0,
                HOLD_MAX_DURATION_MS,
                "duration_ms",
            )?;
            if obj.get("duration_ms").is_none() {
                return Err(invalid("key_hold.duration_ms is required".into()));
            }
            Ok(Action::KeyHold { key, duration_ms })
        }
        _ => unreachable!("unsupported kind handled above"),
    }
}

fn parse_position(value: Option<&serde_json::Value>) -> Result<[i32; 2], ToolError> {
    let invalid = |msg: &str| ToolError::new(codes::INVALID_ACTION, msg.to_string());
    let arr = value
        .and_then(|v| v.as_array())
        .ok_or_else(|| invalid("position must be an array [x, y]"))?;
    if arr.len() != 2 {
        return Err(invalid("position must be an array [x, y]"));
    }
    let x = arr[0]
        .as_i64()
        .ok_or_else(|| invalid("position coordinates must be integers"))?;
    let y = arr[1]
        .as_i64()
        .ok_or_else(|| invalid("position coordinates must be integers"))?;
    let x = i32::try_from(x).map_err(|_| invalid("position coordinate out of range"))?;
    let y = i32::try_from(y).map_err(|_| invalid("position coordinate out of range"))?;
    Ok([x, y])
}

fn parse_button(value: Option<&serde_json::Value>) -> Result<MouseButton, ToolError> {
    match value {
        None => Ok(MouseButton::Left),
        Some(v) => match v.as_str() {
            Some("left") => Ok(MouseButton::Left),
            Some("right") => Ok(MouseButton::Right),
            Some("middle") => Ok(MouseButton::Middle),
            _ => Err(ToolError::new(
                codes::INVALID_ACTION,
                "button must be one of left/right/middle",
            )),
        },
    }
}

fn parse_i32(value: Option<&serde_json::Value>, name: &str) -> Result<i32, ToolError> {
    let v = value.and_then(|v| v.as_i64()).ok_or_else(|| {
        ToolError::new(codes::INVALID_ACTION, format!("{name} must be an integer"))
    })?;
    i32::try_from(v)
        .map_err(|_| ToolError::new(codes::INVALID_ACTION, format!("{name} out of range")))
}

fn parse_duration(
    value: Option<&serde_json::Value>,
    default: u64,
    max: u64,
    name: &str,
) -> Result<u64, ToolError> {
    let v = match value {
        None => default,
        Some(v) => v.as_u64().ok_or_else(|| {
            ToolError::new(
                codes::INVALID_ACTION,
                format!("{name} must be a non-negative integer"),
            )
        })?,
    };
    if v > max {
        return Err(ToolError::new(
            codes::INVALID_ACTION,
            format!("{name} must be at most {max} ms, got {v}"),
        ));
    }
    Ok(v)
}

/// Normalize and validate a key name against the protocol vocabulary.
/// Returns the canonical lowercase name.
///
/// The vocabulary here must stay exactly in lockstep with
/// `crate::backend::keys::parse_key` (enforced by
/// `runtime_key_vocabulary_matches_backend_vocabulary`): names accepted here
/// are normalized and handed to the backend for injection; anything the
/// backend would accept but this layer rejects can never reach injection.
pub fn parse_key_name(raw: Option<&str>) -> Result<String, ToolError> {
    let invalid = |msg: String| ToolError::new(codes::INVALID_ACTION, msg);
    let raw = raw.ok_or_else(|| invalid("key must be a string".into()))?;
    let name = raw.trim().to_ascii_lowercase();
    let ok = match name.as_str() {
        "ctrl" | "control" | "shift" | "alt" | "option" | "meta" | "cmd" | "command" | "win"
        | "super" | "enter" | "return" | "tab" | "space" | "backspace" | "delete" | "escape"
        | "esc" | "up" | "down" | "left" | "right" | "home" | "end" | "pageup" | "page_up"
        | "pagedown" | "page_down" => true,
        _ => {
            (name.len() == 1 && name.chars().next().unwrap().is_ascii_alphanumeric())
                // f1..f12: one or two digits, e.g. f1, f9, f10, f12.
                || (name.len() >= 2
                    && name.len() <= 3
                    && name.starts_with('f')
                    && name[1..].chars().all(|c| c.is_ascii_digit())
                    && name[1..]
                        .parse::<u32>()
                        .map(|n| (1..=12).contains(&n))
                        .unwrap_or(false))
        }
    };
    if ok {
        Ok(name)
    } else {
        Err(invalid(format!("unknown key name: {raw}")))
    }
}

fn is_modifier(name: &str) -> bool {
    matches!(
        name,
        "ctrl"
            | "control"
            | "shift"
            | "alt"
            | "option"
            | "meta"
            | "cmd"
            | "command"
            | "win"
            | "super"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse(v: serde_json::Value) -> Result<Action, ToolError> {
        parse_action(&v)
    }

    #[test]
    fn click_defaults() {
        let a = parse(json!({"kind": "click", "position": [10, 20]})).unwrap();
        assert_eq!(
            a,
            Action::Click {
                position: [10, 20],
                button: MouseButton::Left,
                count: 1
            }
        );
    }

    #[test]
    fn click_count_bounds() {
        assert!(parse(json!({"kind":"click","position":[0,0],"count":0})).is_err());
        assert!(parse(json!({"kind":"click","position":[0,0],"count":4})).is_err());
        assert!(parse(json!({"kind":"click","position":[0,0],"count":3})).is_ok());
    }

    #[test]
    fn unknown_kind_is_unsupported() {
        let e = parse(json!({"kind":"hack"})).unwrap_err();
        assert_eq!(e.code, codes::UNSUPPORTED_ACTION);
    }

    #[test]
    fn unknown_field_rejected() {
        let e = parse(json!({"kind":"click","position":[0,0],"coordinate":[0,0]})).unwrap_err();
        assert_eq!(e.code, codes::INVALID_ACTION);
    }

    #[test]
    fn scroll_requires_unit_and_bounds() {
        assert!(parse(json!({"kind":"scroll","position":[0,0],"delta_x":0,"delta_y":1})).is_err());
        let e = parse(
            json!({"kind":"scroll","position":[0,0],"delta_x":0,"delta_y":1,"unit":"pixels"}),
        )
        .unwrap_err();
        assert_eq!(e.code, codes::UNSUPPORTED_ACTION);
        assert!(parse(
            json!({"kind":"scroll","position":[0,0],"delta_x":101,"delta_y":0,"unit":"wheel_ticks"})
        )
        .is_err());
        assert!(parse(
            json!({"kind":"scroll","position":[0,0],"delta_x":0,"delta_y":0,"unit":"wheel_ticks"})
        )
        .is_err());
    }

    #[test]
    fn text_limit_counts_chars() {
        let long = "汉".repeat(TEXT_MAX_CHARS + 1);
        assert!(parse(json!({"kind":"text_input","text":long})).is_err());
        let ok = "汉".repeat(TEXT_MAX_CHARS);
        assert!(parse(json!({"kind":"text_input","text":ok})).is_ok());
    }

    #[test]
    fn chord_validation() {
        assert!(parse(json!({"kind":"key_chord","modifiers":[],"key":"a"})).is_err());
        assert!(parse(json!({"kind":"key_chord","modifiers":["ctrl"],"key":"ctrl"})).is_err());
        assert!(parse(json!({"kind":"key_chord","modifiers":["ctrl","ctrl"],"key":"a"})).is_err());
        assert!(parse(json!({"kind":"key_chord","modifiers":["a"],"key":"b"})).is_err());
        assert!(parse(json!({"kind":"key_chord","modifiers":["ctrl"],"key":"shift"})).is_err());
        assert!(parse(json!({"kind":"key_chord","modifiers":["ctrl","shift"],"key":"s"})).is_ok());
        assert!(parse(json!({"kind":"key_chord","modifiers":["ctrl"],"key":"f13"})).is_err());
    }

    #[test]
    fn hold_bounds() {
        assert!(parse(json!({"kind":"key_hold","key":"shift"})).is_err());
        assert!(parse(json!({"kind":"key_hold","key":"shift","duration_ms":5001})).is_err());
        assert!(parse(json!({"kind":"key_hold","key":"shift","duration_ms":5000})).is_ok());
    }

    #[test]
    fn drag_validation() {
        assert!(parse(json!({"kind":"drag","path":[[0,0]]})).is_err());
        assert!(parse(json!({"kind":"drag","path":[[0,0],[1,1]],"duration_ms":5001})).is_err());
        let a = parse(json!({"kind":"drag","path":[[0,0],[100,0]]})).unwrap();
        assert_eq!(
            a,
            Action::Drag {
                path: vec![[0, 0], [100, 0]],
                button: MouseButton::Left,
                duration_ms: DRAG_DEFAULT_DURATION_MS
            }
        );
    }

    #[test]
    fn key_name_vocabulary() {
        for ok in [
            "ctrl",
            "CONTROL",
            "shift",
            "alt",
            "option",
            "meta",
            "cmd",
            "command",
            "win",
            "enter",
            "return",
            "tab",
            "space",
            "backspace",
            "delete",
            "escape",
            "esc",
            "up",
            "down",
            "left",
            "right",
            "home",
            "end",
            "pageup",
            "pagedown",
            "f1",
            "f10",
            "f11",
            "f12",
            "a",
            "Z",
            "0",
            "9",
        ] {
            assert!(parse_key_name(Some(ok)).is_ok(), "should accept {ok}");
        }
        for bad in [
            "f0", "f13", "f1x", "fx1", "ab", "", "ctrl+l", "🦀", "capslock",
        ] {
            assert!(parse_key_name(Some(bad)).is_err(), "should reject {bad}");
        }
    }

    #[test]
    fn runtime_key_vocabulary_matches_backend_vocabulary() {
        // The runtime prevalidates names before any injection; the backend
        // validates again at the injection boundary. These two vocabularies
        // must agree exactly, or a name could pass runtime validation and
        // fail mid-plan (leaving a chord half-pressed).
        for name in [
            "ctrl",
            "control",
            "shift",
            "alt",
            "option",
            "meta",
            "cmd",
            "command",
            "win",
            "enter",
            "return",
            "tab",
            "space",
            "backspace",
            "delete",
            "escape",
            "esc",
            "up",
            "down",
            "left",
            "right",
            "home",
            "end",
            "pageup",
            "pagedown",
            "f1",
            "f2",
            "f3",
            "f4",
            "f5",
            "f6",
            "f7",
            "f8",
            "f9",
            "f10",
            "f11",
            "f12",
            "a",
            "z",
            "0",
            "9",
        ] {
            assert_eq!(
                parse_key_name(Some(name)).is_ok(),
                crate::backend::keys::parse_key(name).is_ok(),
                "vocabulary mismatch for {name}"
            );
        }
        for bad in [
            "f0",
            "f13",
            "ab",
            "",
            "ctrl+l",
            "é",
            "super",
            "page_up",
            "page_down",
        ] {
            assert_eq!(
                parse_key_name(Some(bad)).is_ok(),
                crate::backend::keys::parse_key(bad).is_ok(),
                "rejection mismatch for {bad}"
            );
        }
    }
}
