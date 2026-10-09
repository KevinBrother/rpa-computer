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
        /// Native click-state of THIS press/up pair (1..=3).
        click_count: u8,
    },
    Key {
        key: String,
        direction: Direction,
    },
    /// One Unicode scalar for the native text path (see `compile_plan`'s
    /// TextInput branch). Enter/Tab mapping happens in the backend.
    TextScalar(char),
    Scroll {
        x: i32,
        y: i32,
    },
    /// Interruptible, cancellation-checked sleep.
    Sleep(u64),
}

/// Interruptible pause (ms) between consecutive text scalars. This is the
/// DEFAULT interval that keeps cancellation responsive between characters —
/// a candidate, not a guarantee: the executor's cancellation check runs at
/// event boundaries and the input-phase deadline still bounds the whole
/// phase, so long texts can still be cut short and are reported partial
/// truthfully.
pub const TEXT_SCALAR_INTERVAL_MS: u64 = 1;

/// Normalize CRLF (`"\r\n"`) to a single `'\n'` so one newline produces
/// exactly ONE Return click downstream. Lone `'\n'` and lone `'\r'` are kept
/// (each maps to one Return click). Pure, order/content preserving.
pub fn normalize_newlines(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\r' && chars.peek() == Some(&'\n') {
            chars.next(); // consume the pair
            out.push('\n');
        } else {
            out.push(ch);
        }
    }
    out
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
        let release = match self.events.get(index) {
            Some(
                event @ PlanEvent::Button {
                    direction: Direction::Release,
                    ..
                },
            )
            | Some(
                event @ PlanEvent::Key {
                    direction: Direction::Release,
                    ..
                },
            ) => event,
            _ => return false,
        };
        // Find the last event for THIS pair, not just any earlier press.
        // Otherwise cancellation during click 2 can emit click 3's up event
        // even though click 3 was never pressed (or re-release click 1).
        self.events[..index]
            .iter()
            .enumerate()
            .rev()
            .find_map(|(p, event)| {
                let direction = match (release, event) {
                    (
                        PlanEvent::Button {
                            button,
                            click_count,
                            ..
                        },
                        PlanEvent::Button {
                            button: prior_button,
                            click_count: prior_count,
                            direction,
                        },
                    ) if button == prior_button && click_count == prior_count => direction,
                    (
                        PlanEvent::Key { key, .. },
                        PlanEvent::Key {
                            key: prior_key,
                            direction,
                        },
                    ) if key == prior_key => direction,
                    _ => return None,
                };
                Some(*direction == Direction::Press && pressed.contains(&p))
            })
            .unwrap_or(false)
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
            PlanEvent::Button {
                button,
                direction,
                click_count,
            } => Some(InputEvent::Button {
                button: button.clone(),
                direction: *direction,
                click_count: *click_count,
            }),
            PlanEvent::Key { key, direction } => Some(InputEvent::Key {
                key: key.clone(),
                direction: *direction,
            }),
            PlanEvent::TextScalar(ch) => Some(InputEvent::Text {
                text: ch.to_string(),
            }),
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
                // Pair i carries native click-state i+1: THIS press/up pair's
                // multi-click position in the OS click sequence, not an
                // instruction for a driver loop.
                events.push(PlanEvent::Button {
                    button: button.name().to_string(),
                    direction: Direction::Press,
                    click_count: i + 1,
                });
                events.push(PlanEvent::Button {
                    button: button.name().to_string(),
                    direction: Direction::Release,
                    click_count: i + 1,
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
                click_count: 1,
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
                click_count: 1,
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
            // One event per Unicode scalar with an interruptible Sleep
            // BETWEEN scalars (none after the last), so cancellation lands
            // between characters and long texts stay bounded per event.
            // CRLF is normalized once here (single Return click per pair).
            let normalized = normalize_newlines(text);
            for (i, ch) in normalized.chars().enumerate() {
                if i > 0 {
                    events.push(PlanEvent::Sleep(TEXT_SCALAR_INTERVAL_MS));
                }
                events.push(PlanEvent::TextScalar(ch));
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
    fn text_is_split_into_per_scalar_events_with_intervals_between() {
        let text = "abc".repeat(500); // 1500 scalars
        let action = Action::TextInput { text: text.clone() };
        let plan = compile_plan(&action, 1);
        let scalars = plan
            .events
            .iter()
            .filter(|e| matches!(e, PlanEvent::TextScalar(_)))
            .count();
        let sleeps = plan
            .events
            .iter()
            .filter(|e| matches!(e, PlanEvent::Sleep(TEXT_SCALAR_INTERVAL_MS)))
            .count();
        assert_eq!(scalars, text.chars().count());
        assert_eq!(
            sleeps,
            text.chars().count() - 1,
            "no sleep after the last scalar"
        );
        // Reconstruct: scalars in order, no other input events.
        let mut rebuilt = String::new();
        for e in &plan.events {
            match e {
                PlanEvent::TextScalar(ch) => rebuilt.push(*ch),
                PlanEvent::Sleep(_) => {}
                other => panic!("unexpected event in text plan: {other:?}"),
            }
        }
        assert_eq!(rebuilt, text);
    }

    #[test]
    fn text_planning_normalizes_crlf_once() {
        let action = Action::TextInput {
            text: "a\r\nb\nc\rd".into(),
        };
        let plan = compile_plan(&action, 1);
        let scalars: Vec<char> = plan
            .events
            .iter()
            .filter_map(|e| match e {
                PlanEvent::TextScalar(ch) => Some(*ch),
                _ => None,
            })
            .collect();
        assert_eq!(scalars, vec!['a', '\n', 'b', '\n', 'c', '\r', 'd']);
    }

    #[test]
    fn text_planning_preserves_unicode_boundaries() {
        let action = Action::TextInput {
            text: "汉漢字".into(),
        };
        let plan = compile_plan(&action, 1);
        let scalars: Vec<char> = plan
            .events
            .iter()
            .filter_map(|e| match e {
                PlanEvent::TextScalar(ch) => Some(*ch),
                _ => None,
            })
            .collect();
        assert_eq!(scalars, vec!['汉', '漢', '字']);
    }

    #[test]
    fn short_text_is_one_scalar_with_no_interval() {
        let action = Action::TextInput { text: "x".into() };
        let plan = compile_plan(&action, 1);
        assert_eq!(plan.events.len(), 1);
        assert!(matches!(&plan.events[0], PlanEvent::TextScalar('x')));
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

    /// count=N must emit N press/up pairs whose click_count metadata is
    /// 1..=N (the native click-state of EACH pair), not 1 repeated N times.
    #[test]
    fn multi_click_pairs_carry_increasing_click_count() {
        let action = Action::Click {
            position: [3, 4],
            button: MouseButton::Left,
            count: 3,
        };
        let plan = compile_plan(&action, 1);
        let click_counts: Vec<u8> = plan
            .events
            .iter()
            .filter_map(|e| match e {
                PlanEvent::Button { click_count, .. } => Some(*click_count),
                _ => None,
            })
            .collect();
        assert_eq!(
            click_counts,
            vec![1, 1, 2, 2, 3, 3],
            "press/up pair i must carry native click-state i, not a constant 1"
        );
    }

    #[test]
    fn single_click_and_drag_carry_click_count_one() {
        let click = compile_plan(
            &Action::Click {
                position: [0, 0],
                button: MouseButton::Left,
                count: 1,
            },
            1,
        );
        assert!(click.events.iter().all(|e| match e {
            PlanEvent::Button { click_count, .. } => *click_count == 1,
            _ => true,
        }));
        let drag = compile_plan(
            &Action::Drag {
                path: vec![[0, 0], [10, 10]],
                button: MouseButton::Left,
                duration_ms: 100,
            },
            1,
        );
        assert!(
            drag.events.iter().all(|e| match e {
                PlanEvent::Button { click_count, .. } => *click_count == 1,
                _ => true,
            }),
            "drag press/release is always click-state 1"
        );
    }
    #[test]
    fn release_pass_only_answers_the_same_successfully_pressed_pair() {
        let plan = compile_plan(
            &Action::Click {
                position: [0, 0],
                button: MouseButton::Left,
                count: 3,
            },
            1,
        );
        // move, down1, up1, gap, down2, up2, gap, down3, up3.
        assert!(plan.is_answered_release(2, &[1]));
        assert!(!plan.is_answered_release(5, &[1]));
        assert!(plan.is_answered_release(5, &[1, 4]));
        assert!(!plan.is_answered_release(8, &[1, 4]));
        assert!(plan.is_answered_release(8, &[1, 4, 7]));
        // A failed chord key must not borrow its modifier's successful press.
        let chord = compile_plan(
            &Action::KeyChord {
                modifiers: vec!["ctrl".into()],
                key: "s".into(),
            },
            1,
        );
        assert!(!chord.is_answered_release(2, &[0]));
        assert!(chord.is_answered_release(3, &[0]));
    }

    #[test]
    fn counts_one_two_three_map_to_backend_with_runtime_owned_gaps() {
        for count in 1..=3 {
            let plan = compile_plan(
                &Action::Click {
                    position: [9, 12],
                    button: MouseButton::Right,
                    count,
                },
                1,
            );
            let expected: Vec<_> = (1..=count)
                .flat_map(|click_count| {
                    [Direction::Press, Direction::Release].map(|direction| InputEvent::Button {
                        button: "right".into(),
                        direction,
                        click_count,
                    })
                })
                .collect();
            let actual: Vec<_> = (0..plan.events.len())
                .filter_map(|i| {
                    plan.backend_event(i, |[x, y]| (x, y))
                        .filter(|e| matches!(e, InputEvent::Button { .. }))
                })
                .collect();
            assert_eq!(actual, expected);
            assert_eq!(
                plan.events
                    .iter()
                    .filter(|e| matches!(e,
                PlanEvent::Sleep(ms) if *ms == MULTI_CLICK_INTERVAL_MS))
                    .count(),
                usize::from(count - 1)
            );
        }
    }
}
