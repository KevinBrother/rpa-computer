//! Full Runtime -> root DisplayProvider -> real pure composition/mapping seam.
//! Only memory FrameProvider/inject/release. No native/GUI/CLI is invoked.
use super::*;
use crate::backend::display::test_support::{facts, MemoryDesktop, Shared};
use crate::backend::{Direction, InputEvent};

fn setup() -> (Runtime, Shared) {
    let shared = facts();
    let rt = Runtime::new(
        Box::new(MemoryDesktop::new(shared.clone())),
        Arc::new(AtomicBool::new(false)),
    );
    (rt, shared)
}
fn desktop(rt: &mut Runtime) -> (String, String) {
    let opened = rt.call(
        "computer_open",
        json!({"display":{"kind":"desktop"},"max_width":400,"max_height":300}),
    );
    assert!(!opened.is_error, "{}", opened.data);
    assert_eq!(opened.data["surface_id"], "desktop");
    let sid = opened.data["session_id"].as_str().unwrap().to_owned();
    let obs = rt.call("computer_observe", json!({"session_id":sid}));
    assert!(!obs.is_error, "{}", obs.data);
    assert_eq!(obs.data["selected_display_ids"], json!(["left", "right"]));
    (sid, obs.data["observation_id"].as_str().unwrap().to_owned())
}
fn step(rt: &mut Runtime, sid: &str, basis: &str, id: &str, action: Value) -> Reply {
    rt.call(
        "computer_step",
        json!({"session_id":sid,"based_on":basis,"request_id":id,"action":action}),
    )
}
#[test]
fn malformed_selectors_reject_before_backend_or_state_mutation() {
    let (mut rt, f) = setup();
    for display in [
        json!(null),
        json!("primary"),
        json!({}),
        json!({"kind":"id"}),
        json!({"kind":"id","id":""}),
        json!({"kind":"bad"}),
        json!({"kind":"primary","id":"right"}),
        json!({"kind":"desktop","extra":1}),
    ] {
        let reply = rt.call("computer_open", json!({"display":display}));
        assert_eq!(reply.data["error"]["code"], "invalid_arguments");
    }
    assert_eq!(f.borrow().queries, 0);
    assert_eq!(f.borrow().captures, 0);
    assert!(f.borrow().events.is_empty());
    let reply = rt.call(
        "computer_open",
        json!({"display":{"kind":"id","id":"absent"}}),
    );
    assert_eq!(reply.data["error"]["code"], "display_not_found");
    let good = rt.call(
        "computer_open",
        json!({"display":{"kind":"id","id":"right"}}),
    );
    assert!(!good.is_error, "{}", good.data);
    assert_eq!(good.data["surface_id"], "right");
}
#[test]
fn describe_is_fresh_complete_physical_topology_without_capture() {
    let (mut rt, f) = setup();
    let d = rt.call("computer_describe", json!({}));
    assert_eq!(d.data["native_unit"], "physical_pixels");
    assert_eq!(d.data["displays"].as_array().unwrap().len(), 2);
    assert_eq!(
        d.data["display_topology"]["topology_generation"],
        d.data["topology_generation"]
    );
    assert_eq!(f.borrow().captures, 0);
    f.borrow_mut().query_fail = true;
    let d = rt.call("computer_describe", json!({}));
    assert_eq!(d.data["available"], false);
    assert!(d.data["display_topology"].is_null());
    assert!(d.data.get("displays").is_none());
    assert!(d.data.get("topology_generation").is_none());
}
#[test]
fn gap_and_outside_all_pointer_actions_reject_before_input() {
    let (mut rt, f) = setup();
    let (sid, obs) = desktop(&mut rt);
    for (i, action) in [
        json!({"kind":"click","position":[240,100]}),
        json!({"kind":"move","position":[240,100]}),
        json!({"kind":"scroll","position":[240,100],"delta_x":0,"delta_y":1,"unit":"wheel_ticks"}),
        json!({"kind":"drag","path":[[100,100],[240,100],[340,180]],"duration_ms":60}),
        json!({"kind":"click","position":[400,100]}),
    ]
    .into_iter()
    .enumerate()
    {
        let reply = step(&mut rt, &sid, &obs, &format!("invalid-{i}"), action);
        assert_eq!(reply.data["input_outcome"], "not_started", "{}", reply.data);
        assert_eq!(reply.data["error"]["code"], "invalid_action");
    }
    assert!(f.borrow().events.is_empty());
}
#[test]
fn pointer_mapping_and_idempotent_replay_use_exact_native_region() {
    let (mut rt, f) = setup();
    let (sid, obs) = desktop(&mut rt);
    let action = json!({"kind":"click","position":[399,299]});
    let first = step(&mut rt, &sid, &obs, "edge", action.clone());
    assert!(!first.is_error, "{}", first.data);
    assert_eq!(f.borrow().events[0], InputEvent::Move { x: 99, y: 149 });
    let count = f.borrow().events.len();
    let repeated = step(&mut rt, &sid, &obs, "edge", action);
    assert_eq!(repeated.data, first.data);
    assert_eq!(f.borrow().events.len(), count);
}
#[test]
fn full_generation_stale_in_unselected_monitor_and_fresh_open_recovers() {
    let (mut rt, f) = setup();
    let sid = open(&mut rt);
    let (basis, _) = observe(&mut rt, &sid);
    f.borrow_mut().displays[1].scale = 2.25; // primary geometry stays identical
    let stale = step(
        &mut rt,
        &sid,
        &basis,
        "stale",
        json!({"kind":"move","position":[10,10]}),
    );
    assert_eq!(stale.data["error"]["code"], "geometry_changed");
    assert_eq!(stale.data["input_outcome"], "not_started");
    assert!(f.borrow().events.is_empty());
    assert!(
        !rt.call("computer_close", json!({"session_id":sid}))
            .is_error
    );
    let sid = open(&mut rt);
    let (basis, _) = observe(&mut rt, &sid);
    assert!(
        !step(
            &mut rt,
            &sid,
            &basis,
            "fresh",
            json!({"kind":"move","position":[10,10]})
        )
        .is_error
    );
}
#[test]
fn pause_resume_rejects_full_generation_change() {
    let (mut rt, f) = setup();
    let (sid, _) = desktop(&mut rt);
    assert!(
        !rt.call("computer_pause", json!({"session_id":sid}))
            .is_error
    );
    f.borrow_mut().displays[1].rotation = rpa_display_topology::Rotation::Deg90;
    let reply = rt.call("computer_resume", json!({"session_id":sid}));
    assert_eq!(reply.data["error"]["code"], "geometry_changed");
    assert!(f.borrow().events.is_empty());
}
#[test]
fn cross_screen_drag_transits_gap_but_ends_on_real_display() {
    let (mut rt, f) = setup();
    let (sid, obs) = desktop(&mut rt);
    let reply = step(
        &mut rt,
        &sid,
        &obs,
        "drag",
        json!({"kind":"drag","path":[[100,100],[340,180]],"duration_ms":100}),
    );
    assert!(!reply.is_error, "{}", reply.data);
    let f = f.borrow();
    assert!(
        f.events
            .iter()
            .any(|e| matches!(e,InputEvent::Move{x,..} if *x>=0 && *x<40)),
        "must cross gap internally"
    );
    assert!(f.events.contains(&InputEvent::Move { x: 70, y: 90 }));
    assert!(matches!(
        f.events.last(),
        Some(InputEvent::Button {
            direction: Direction::Release,
            ..
        })
    ));
}
#[test]
fn topology_change_or_query_error_mid_drag_stops_and_releases_truthfully() {
    for query_failure in [false, true] {
        for release_failure in [false, true] {
            let (mut rt, f) = setup();
            let (sid, obs) = desktop(&mut rt);
            if query_failure {
                f.borrow_mut().fail_after_events = Some(2);
            } else {
                f.borrow_mut().change_after_events = Some(2);
            }
            f.borrow_mut().release_fail = release_failure;
            let reply = step(
                &mut rt,
                &sid,
                &obs,
                "changed",
                json!({"kind":"drag","path":[[100,100],[340,180]],"duration_ms":100}),
            );
            assert!(reply.is_error);
            assert_eq!(reply.data["input_outcome"], "partial");
            assert_eq!(reply.data["observation_outcome"], "skipped");
            assert_eq!(
                reply.data["cleanup_outcome"],
                if release_failure {
                    "failed"
                } else {
                    "released"
                }
            );
            assert_eq!(reply.data["cancelled"], true);
            assert_eq!(
                f.borrow()
                    .events
                    .iter()
                    .filter(|e| matches!(e, InputEvent::Move { .. }))
                    .count(),
                1
            );
            assert!(matches!(
                f.borrow().events.last(),
                Some(InputEvent::Button {
                    direction: Direction::Release,
                    ..
                })
            ));
            assert_eq!(f.borrow().releases, 1);
        }
    }
}

#[test]
fn cancellation_mid_cross_screen_drag_releases_without_later_moves() {
    let f = facts();
    let cancel = Arc::new(AtomicBool::new(false));
    let mut rt = Runtime::new(Box::new(MemoryDesktop::new(f.clone())), cancel.clone());
    let (sid, obs) = desktop(&mut rt);
    f.borrow_mut().cancel_after_events = Some((2, cancel));
    let reply = step(
        &mut rt,
        &sid,
        &obs,
        "cancel",
        json!({"kind":"drag","path":[[100,100],[340,180]],"duration_ms":100}),
    );
    assert_eq!(reply.data["input_outcome"], "partial");
    assert_eq!(reply.data["cancelled"], true);
    assert_eq!(reply.data["cleanup_outcome"], "released");
    assert_eq!(
        f.borrow()
            .events
            .iter()
            .filter(|e| matches!(e, InputEvent::Move { .. }))
            .count(),
        1
    );
    assert_eq!(f.borrow().releases, 1);
}
#[test]
fn provider_capture_error_does_not_erase_dispatched_input() {
    let (mut rt, f) = setup();
    let (sid, obs) = desktop(&mut rt);
    f.borrow_mut().capture_fail = true;
    let reply = step(
        &mut rt,
        &sid,
        &obs,
        "capture-error",
        json!({"kind":"move","position":[340,180]}),
    );
    assert!(reply.is_error);
    assert_eq!(reply.data["input_outcome"], "dispatched");
    assert_eq!(reply.data["observation_outcome"], "failed");
    assert_eq!(f.borrow().events[0], InputEvent::Move { x: 70, y: 90 });
}
#[test]
fn real_provider_png_still_obeys_existing_session_cache_budget() {
    use crate::backend::Backend;
    let f = facts();
    let mut backend = MemoryDesktop::new(f.clone());
    let geometry = backend.geometry().unwrap();
    let mut session = crate::runtime::session::Session::new("cache".into(), geometry, (400, 300));
    session.set_image_byte_budget(16);
    let error = crate::runtime::session::capture_observation(
        &mut session,
        &mut backend,
        0,
        &Arc::new(AtomicBool::new(false)),
    )
    .unwrap_err();
    assert_eq!(error.code, "resource_limit");
    assert!(f.borrow().events.is_empty());
}
