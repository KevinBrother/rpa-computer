//! Pure step planning: a validated [`Action`] compiles to a [`Plan`] of
//! atomic [`PlannedEvent`]s. Execution (deadlines, cancellation, chunked
//! text) is provided by the generic driver in `crate::runtime::execute`,
//! which the session layer uses.

use crate::backend::{Direction, InputEvent};
use crate::runtime::actions::{Action, DRAG_MOVE_INTERVAL_MS, MULTI_CLICK_INTERVAL_MS};

/// One planned primitive, still in image coordinates.
#[derive(Debug, Clone, PartialEq)]
pub enum PlanEvent {
    Move([i32; 2]),
    Button {
        button: String,
        direction: Direction,
    },
    Key {
        key: String,
        direction: Direction,
    },
    /// One bounded chunk of text (see `chunk_text`). Text actions never
    /// compile to a single multi-thousand-character event, so a native
    /// text() call stays short and cancellation between events is honored.
    Text(String),
    Scroll {
        x: i32,
        y: i32,
    },
    /// Interruptible, cancellation-checked sleep.
    Sleep(u64),
}

/// Maximum characters handed to the backend in a single `Text` event. Small
/// enough that one native text() call is brief and a pause/close lands
/// within the cooperative-stop budget between events.
pub const TEXT_CHUNK_CHARS: usize = 64;

/// Split text into bounded chunks on Unicode scalar boundaries. Order and
/// content are preserved exactly (concatenating the chunks yields the input).
pub fn chunk_text(text: &str, max_chars: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        current.push(ch);
        if current.chars().count() >= max_chars {
            chunks.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

/// The compiled execution plan for one action: ordered events plus the
/// half-open index range `[hold_press, hold_end)` during which keys/buttons
/// may be physically held. Events in that range (sleeps between press and
/// release, intermediate drag moves, multi-click gaps) must never claim the
/// full requested duration if the input-phase deadline cuts them short —
/// the executor shortens them and reports the hold as partial.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub events: Vec<PlanEvent>,
    pub hold_press: Option<usize>,
    pub hold_end: usize,
}

impl Plan {
    /// True if the event at `index` injects input into the OS (as opposed to
    /// a local sleep). Used to attribute partial failures and cleanup.
    pub fn is_input_event(&self, index: usize) -> bool {
        !matches!(self.events.get(index), Some(PlanEvent::Sleep(_)) | None)
    }

    /// True if the event at `index` is a key/button press (the "down" half a
    /// later release would answer).
    pub fn is_press_event(&self, index: usize) -> bool {
        matches!(
            self.events.get(index),
            Some(PlanEvent::Button {
                direction: Direction::Press,
                ..
            }) | Some(PlanEvent::Key {
                direction: Direction::Press,
                ..
            })
        )
    }

    /// True if the event at `index` is a release answering a press that was
    /// actually injected (`pressed` holds the plan indexes of successful
    /// press events). Only such a release is safe to send on a failure path:
    /// releasing a key/button whose press failed or was never attempted can
    /// unlock UI state (window drag, menu, modifier) the user owns.
    pub fn is_answered_release(&self, index: usize, pressed: &[usize]) -> bool {
        let is_release = matches!(
            self.events.get(index),
            Some(PlanEvent::Button {
                direction: Direction::Release,
                ..
            }) | Some(PlanEvent::Key {
                direction: Direction::Release,
                ..
            })
        );
        if !is_release {
            return false;
        }
        // Every earlier press was injected successfully; a failed press would
        // have stopped the dispatch loop before reaching this release.
        pressed.iter().any(|&p| p < index)
    }

    /// Convert the planned event at `index` into a backend `InputEvent`,
    /// mapping an image-space position through `map` when needed.
    pub fn backend_event(
        &self,
        index: usize,
        map: impl Fn([i32; 2]) -> (i32, i32),
    ) -> Option<InputEvent> {
        match self.events.get(index)? {
            PlanEvent::Move(p) => {
                let (x, y) = map(*p);
                Some(InputEvent::Move { x, y })
            }
            PlanEvent::Button { button, direction } => Some(InputEvent::Button {
                button: button.clone(),
                direction: *direction,
            }),
            PlanEvent::Key { key, direction } => Some(InputEvent::Key {
                key: key.clone(),
                direction: *direction,
            }),
            PlanEvent::Text(t) => Some(InputEvent::Text { text: t.clone() }),
            PlanEvent::Scroll { x, y } => Some(InputEvent::Scroll { x: *x, y: *y }),
            PlanEvent::Sleep(_) => None,
        }
    }
}

/// Compile a validated action into a chunked, hold-annotated plan.
/// `drag_moves` is the number of intermediate move events along a drag path.
pub fn compile_plan(action: &Action, drag_moves: usize) -> Plan {
    let mut events = Vec::new();
    let mut hold_press = None;
    let mut hold_end = 0;
    match action {
        Action::Click {
            position,
            button,
            count,
        } => {
            events.push(PlanEvent::Move(*position));
            for i in 0..*count {
                if i > 0 {
                    events.push(PlanEvent::Sleep(MULTI_CLICK_INTERVAL_MS));
                }
                events.push(PlanEvent::Button {
                    button: button.name().to_string(),
                    direction: Direction::Press,
                });
                events.push(PlanEvent::Button {
                    button: button.name().to_string(),
                    direction: Direction::Release,
                });
            }
            if *count > 0 {
                hold_press = Some(1); // after the initial Move
                hold_end = events.len();
            }
        }
        Action::Move { position } => {
            events.push(PlanEvent::Move(*position));
        }
        Action::Drag {
            path,
            button,
            duration_ms,
        } => {
            events.push(PlanEvent::Move(path[0]));
            hold_press = Some(events.len());
            events.push(PlanEvent::Button {
                button: button.name().to_string(),
                direction: Direction::Press,
            });
            let moves = drag_moves.max(1);
            let interval = duration_ms / moves as u64;
            for (i, point) in drag_interpolate(path, moves).into_iter().enumerate() {
                events.push(PlanEvent::Move(point));
                if i + 1 < moves {
                    events.push(PlanEvent::Sleep(interval.max(DRAG_MOVE_INTERVAL_MS)));
                }
            }
            events.push(PlanEvent::Button {
                button: button.name().to_string(),
                direction: Direction::Release,
            });
            hold_end = events.len();
        }
        Action::Scroll {
            position,
            delta_x,
            delta_y,
        } => {
            events.push(PlanEvent::Move(*position));
            events.push(PlanEvent::Scroll {
                x: *delta_x,
                y: *delta_y,
            });
        }
        Action::TextInput { text } => {
            for chunk in chunk_text(text, TEXT_CHUNK_CHARS) {
                events.push(PlanEvent::Text(chunk));
            }
        }
        Action::KeyChord { modifiers, key } => {
            hold_press = Some(events.len());
            for m in modifiers {
                events.push(PlanEvent::Key {
                    key: m.clone(),
                    direction: Direction::Press,
                });
            }
            events.push(PlanEvent::Key {
                key: key.clone(),
                direction: Direction::Press,
            });
            events.push(PlanEvent::Key {
                key: key.clone(),
                direction: Direction::Release,
            });
            for m in modifiers.iter().rev() {
                events.push(PlanEvent::Key {
                    key: m.clone(),
                    direction: Direction::Release,
                });
            }
            hold_end = events.len();
        }
        Action::KeyHold { key, duration_ms } => {
            hold_press = Some(events.len());
            events.push(PlanEvent::Key {
                key: key.clone(),
                direction: Direction::Press,
            });
            events.push(PlanEvent::Sleep(*duration_ms));
            events.push(PlanEvent::Key {
                key: key.clone(),
                direction: Direction::Release,
            });
            hold_end = events.len();
        }
    }
    Plan {
        events,
        hold_press,
        hold_end,
    }
}

/// Walk `path` and emit `moves` interpolated image-space points along it,
/// always ending exactly on the last path point.
fn drag_interpolate(path: &[[i32; 2]], moves: usize) -> Vec<[i32; 2]> {
    if path.len() == 1 {
        return vec![path[0]; moves];
    }
    let seg_lens: Vec<f64> = path
        .windows(2)
        .map(|w| {
            let dx = (w[1][0] - w[0][0]) as f64;
            let dy = (w[1][1] - w[0][1]) as f64;
            (dx * dx + dy * dy).sqrt()
        })
        .collect();
    let total: f64 = seg_lens.iter().sum();
    let mut out = Vec::with_capacity(moves);
    for i in 0..moves {
        let target = total * (i as f64 + 1.0) / moves as f64;
        out.push(point_at(path, &seg_lens, target));
    }
    if let Some(last) = out.last_mut() {
        *last = *path.last().unwrap();
    }
    out
}

fn point_at(path: &[[i32; 2]], seg_lens: &[f64], target: f64) -> [i32; 2] {
    let mut remaining = target;
    for (i, len) in seg_lens.iter().enumerate() {
        if remaining <= *len || i == seg_lens.len() - 1 {
            let t = if *len > 0.0 { remaining / len } else { 0.0 };
            let t = t.clamp(0.0, 1.0);
            let a = path[i];
            let b = path[i + 1];
            return [
                (a[0] as f64 + (b[0] - a[0]) as f64 * t).round() as i32,
                (a[1] as f64 + (b[1] - a[1]) as f64 * t).round() as i32,
            ];
        }
        remaining -= len;
    }
    *path.last().unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::actions::MouseButton;

    #[test]
    fn drag_interpolation_hits_endpoints() {
        let path = vec![[0, 0], [100, 0], [100, 50]];
        let pts = drag_interpolate(&path, 10);
        assert_eq!(pts.len(), 10);
        assert_eq!(*pts.last().unwrap(), [100, 50]);
        // Points must be monotonic along the path (never jump backwards).
        for w in pts.windows(2) {
            assert!(w[1][0] >= w[0][0] && w[1][1] >= w[0][1]);
        }
    }

    #[test]
    fn text_is_split_into_bounded_chunks() {
        let text = "a".repeat(4096);
        let chunks = chunk_text(&text, TEXT_CHUNK_CHARS);
        assert_eq!(chunks.len(), 64);
        assert!(chunks.iter().all(|c| c.chars().count() <= TEXT_CHUNK_CHARS));
        assert_eq!(chunks.concat(), text);
    }

    #[test]
    fn text_chunking_respects_unicode_boundaries() {
        let text = "汉".repeat(100);
        let chunks = chunk_text(&text, 30);
        assert_eq!(chunks.len(), 4);
        assert_eq!(chunks.concat(), text);
        assert!(chunks.iter().all(|c| c.chars().count() <= 30));
    }

    #[test]
    fn short_text_stays_one_event() {
        let action = Action::TextInput {
            text: "hello".into(),
        };
        let plan = compile_plan(&action, 1);
        assert_eq!(plan.events.len(), 1);
        assert!(matches!(&plan.events[0], PlanEvent::Text(t) if t == "hello"));
    }

    #[test]
    fn key_hold_window_covers_press_sleep_release() {
        let action = Action::KeyHold {
            key: "shift".into(),
            duration_ms: 5000,
        };
        let plan = compile_plan(&action, 1);
        assert_eq!(plan.events.len(), 3);
        assert_eq!(plan.hold_press, Some(0));
        assert_eq!(plan.hold_end, 3);
    }

    #[test]
    fn answered_release_detection_requires_prior_press() {
        let plan = compile_plan(
            &Action::KeyChord {
                modifiers: vec!["ctrl".into()],
                key: "s".into(),
            },
            1,
        );
        // ctrl press, s press, s release, ctrl release.
        assert!(!plan.is_answered_release(0, &[0]));
        assert!(!plan.is_answered_release(1, &[0, 1]));
        assert!(plan.is_answered_release(2, &[0, 1]));
        assert!(plan.is_answered_release(3, &[0, 1]));
        // A release with NO prior successful press is never answered.
        assert!(!plan.is_answered_release(3, &[]));

        let hold = compile_plan(
            &Action::KeyHold {
                key: "shift".into(),
                duration_ms: 10,
            },
            1,
        );
        assert!(!hold.is_answered_release(0, &[]));
        assert!(hold.is_answered_release(2, &[0]));

        // No press anywhere: nothing answers.
        let text = compile_plan(
            &Action::TextInput {
                text: "hello".into(),
            },
            1,
        );
        assert!(!text.is_answered_release(0, &[]));
    }

    #[test]
    fn click_count_marks_hold_window() {
        let action = Action::Click {
            position: [1, 2],
            button: MouseButton::Left,
            count: 2,
        };
        let plan = compile_plan(&action, 1);
        // move, press, release, sleep, press, release
        assert_eq!(plan.events.len(), 6);
        assert_eq!(plan.hold_press, Some(1));
        assert_eq!(plan.hold_end, 6);
    }
}
