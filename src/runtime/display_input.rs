//! Compile native per-region moves; only DragPlan samples may transit a gap.
use crate::backend::{Backend, Direction};
use crate::runtime::{
    actions::Action,
    error::{codes, ToolError},
    execute::{self, ExecutionConfig, ExecutionOutcome},
    plan::{self, Plan, PlanEvent},
    session::Observation,
};
use rpa_display_topology::{DragPlan, ImagePoint, ObservationMapping};
use std::sync::{atomic::AtomicBool, Arc};

fn point(p: [i32; 2]) -> ImagePoint {
    ImagePoint {
        x: f64::from(p[0]),
        y: f64::from(p[1]),
    }
}
fn invalid(e: impl std::fmt::Display) -> ToolError {
    ToolError::new(codes::INVALID_ACTION, e.to_string())
}
pub fn contains(basis: &Observation, p: [i32; 2]) -> bool {
    match &basis.regions {
        Some(m) => m.map_image(point(p), m.generation()).is_ok(),
        None => basis.map.contains(p),
    }
}
struct Prepared {
    plan: Plan,
    drag: Option<DragPlan>,
    // Event index, segment, rational sample numerator/denominator.
    samples: Vec<(usize, usize, u32, u32)>,
}
fn prepare(action: &Action, m: &ObservationMapping, moves: usize) -> Result<Prepared, ToolError> {
    if let Action::Drag {
        path,
        button,
        duration_ms,
    } = action
    {
        let waypoints: Vec<_> = path.iter().copied().map(point).collect();
        let drag = DragPlan::from_image(m, &waypoints, m.generation()).map_err(invalid)?;
        let segments = drag.segments().len();
        let moves = moves.max(segments);
        let first = drag.sample(0, 0, 1, m.generation()).map_err(invalid)?;
        let mut events = vec![
            PlanEvent::Move([first.x, first.y]),
            PlanEvent::Button {
                button: button.name().into(),
                direction: Direction::Press,
                click_count: 1,
            },
        ];
        let mut samples = vec![(0, 0, 0, 1)];
        let mut completed = 0;
        for segment in 0..segments {
            let steps = (moves / segments + usize::from(segment < moves % segments)) as u32;
            for step in 1..=steps {
                let p = drag
                    .sample(segment, step, steps, m.generation())
                    .map_err(invalid)?;
                samples.push((events.len(), segment, step, steps));
                events.push(PlanEvent::Move([p.x, p.y]));
                completed += 1;
                if completed < moves {
                    events.push(PlanEvent::Sleep(
                        (duration_ms / moves as u64)
                            .max(crate::runtime::actions::DRAG_MOVE_INTERVAL_MS),
                    ));
                }
            }
        }
        events.push(PlanEvent::Button {
            button: button.name().into(),
            direction: Direction::Release,
            click_count: 1,
        });
        let end = events.len();
        Ok(Prepared {
            plan: Plan {
                events,
                hold_press: Some(1),
                hold_end: end,
            },
            drag: Some(drag),
            samples,
        })
    } else {
        let mut plan = plan::compile_plan(action, moves);
        for e in &mut plan.events {
            if let PlanEvent::Move(p) = e {
                let native = m.map_image(point(*p), m.generation()).map_err(invalid)?;
                *p = [native.x, native.y];
            }
        }
        Ok(Prepared {
            plan,
            drag: None,
            samples: vec![],
        })
    }
}
pub fn execute(
    action: &Action,
    basis: &Observation,
    backend: &mut dyn Backend,
    cancel: &Arc<AtomicBool>,
    config: &ExecutionConfig,
    moves: usize,
) -> ExecutionOutcome {
    let Some(mapping) = &basis.regions else {
        return execute::execute_plan(
            &plan::compile_plan(action, moves),
            backend,
            |p| basis.map.to_native(p),
            cancel,
            config,
        );
    };
    let prepared = match prepare(action, mapping, moves) {
        Ok(p) => p,
        Err(e) => {
            // Error before first input: guarded executor preserves not_started.
            let plan = plan::compile_plan(action, moves);
            return execute::execute_plan_guarded(
                &plan,
                backend,
                |p| (p[0], p[1]),
                cancel,
                config,
                |_, _| Err(e.clone()),
            );
        }
    };
    execute::execute_plan_guarded(
        &prepared.plan,
        backend,
        |p| (p[0], p[1]),
        cancel,
        config,
        |b, index| {
            let latest = crate::runtime::display::check(b, Some(mapping.generation()))
                .map_err(|e| {
                    ToolError::new(
                        codes::GEOMETRY_CHANGED,
                        format!("topology validation failed: {}: {}", e.code, e.message),
                    )
                })?
                .ok_or_else(|| {
                    ToolError::new(codes::GEOMETRY_CHANGED, "missing topology authority")
                })?;
            if let Some(drag) = &prepared.drag {
                if let Some(&(_, segment, step, steps)) =
                    prepared.samples.iter().find(|s| s.0 == index)
                {
                    let p = drag
                        .sample(segment, step, steps, latest.generation())
                        .map_err(invalid)?;
                    if prepared.plan.events[index] != PlanEvent::Move([p.x, p.y]) {
                        return Err(invalid("drag sample mismatch"));
                    }
                }
            }
            Ok(())
        },
    )
}
