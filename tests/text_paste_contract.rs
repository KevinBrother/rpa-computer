//! Pure contract tests: no real clipboard, capture, or desktop input.
//! Windows execution belongs to CC+GLM; mocks do not certify GUI text correctness.
#[path = "contract/support.rs"]
mod support;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rpa_computer::runtime::actions::{parse_action, Action, TEXT_MAX_CHARS};
use rpa_computer::runtime::error::codes;
use rpa_computer::runtime::execute::{execute_plan, execute_plan_guarded, ExecutionConfig};
use rpa_computer::runtime::plan::{compile_plan, PlanEvent, TEXT_SCALAR_INTERVAL_MS};
use rpa_computer::runtime::session::{CleanupOutcome, InputOutcome};
use rpa_computer::runtime::tool_definitions;
use serde_json::{json, Value};
use support::{err, MockBackend};

fn paste(text: &str) -> Action {
    parse_action(&json!({"kind": "text_input", "method": "paste", "text": text})).unwrap()
}

fn config() -> ExecutionConfig {
    ExecutionConfig {
        settle_delay: Duration::ZERO,
        ..ExecutionConfig::production()
    }
}

#[test]
fn omitted_and_explicit_native_keep_existing_action_and_plan() {
    let text = "中🙂\r\n\tA";
    let expected = Action::TextInput { text: text.into() };
    for action in [
        json!({"kind": "text_input", "text": text}),
        json!({"kind": "text_input", "method": "native", "text": text}),
    ] {
        assert_eq!(parse_action(&action).unwrap(), expected);
    }
    assert_eq!(
        compile_plan(&expected, 1).events,
        vec![
            PlanEvent::TextScalar('中'),
            PlanEvent::Sleep(TEXT_SCALAR_INTERVAL_MS),
            PlanEvent::TextScalar('🙂'),
            PlanEvent::Sleep(TEXT_SCALAR_INTERVAL_MS),
            PlanEvent::TextScalar('\n'),
            PlanEvent::Sleep(TEXT_SCALAR_INTERVAL_MS),
            PlanEvent::TextScalar('\t'),
            PlanEvent::Sleep(TEXT_SCALAR_INTERVAL_MS),
            PlanEvent::TextScalar('A'),
        ]
    );
}

#[test]
fn paste_remains_public_text_input_but_has_its_own_action() {
    let action = paste("中文🙂");
    assert_ne!(
        action,
        Action::TextInput {
            text: "中文🙂".into()
        }
    );
    assert_eq!(action.kind(), "text_input");
    assert!(action.positions().is_empty());
    assert_eq!(action.names(), (vec![], vec![]));
    assert!(parse_action(&json!({"kind": "text_paste", "text": "x"})).is_err());
}

#[test]
fn unknown_or_non_string_method_is_rejected_even_for_empty_text() {
    for method in [
        json!("auto"),
        json!("Paste"),
        json!(""),
        Value::Null,
        json!(false),
        json!(0),
        json!([]),
        json!({}),
    ] {
        let error =
            parse_action(&json!({"kind": "text_input", "text": "", "method": method})).unwrap_err();
        assert_eq!(error.code, codes::INVALID_ACTION);
    }
}

#[test]
fn method_does_not_weaken_strict_fields_or_required_text() {
    for action in [
        json!({"kind": "text_input", "method": "paste"}),
        json!({"kind": "text_input", "method": "paste", "text": 1}),
        json!({"kind": "text_input", "method": "paste", "text": "x", "restore": true}),
        json!({"kind": "move", "position": [0, 0], "method": "paste"}),
    ] {
        assert_eq!(
            parse_action(&action).unwrap_err().code,
            codes::INVALID_ACTION
        );
    }
}

#[test]
fn both_methods_prevalidate_every_scalar_before_planning() {
    for method in ["native", "paste"] {
        for control in (0u8..=31).chain(std::iter::once(127)) {
            if [b'\t', b'\r', b'\n'].contains(&control) {
                continue;
            }
            let text = format!("valid prefix中🙂{}suffix", char::from(control));
            let error =
                parse_action(&json!({"kind": "text_input", "method": method, "text": text}))
                    .unwrap_err();
            assert_eq!(error.code, codes::INVALID_ACTION);
        }
    }
}

#[test]
fn both_methods_bound_unicode_scalars_not_utf8_or_utf16_units() {
    for method in ["native", "paste"] {
        for count in [TEXT_MAX_CHARS, TEXT_MAX_CHARS + 1] {
            let result = parse_action(
                &json!({"kind": "text_input", "method": method, "text": "🙂".repeat(count)}),
            );
            assert_eq!(result.is_ok(), count == TEXT_MAX_CHARS);
        }
    }
}

#[test]
fn empty_paste_has_no_backend_effects() {
    let plan = compile_plan(&paste(""), 1);
    assert!(plan.events.is_empty());
    let mut backend = MockBackend::new(1, 1);
    let out = execute_plan(
        &plan,
        &mut backend,
        |p| (p[0], p[1]),
        &Arc::new(AtomicBool::new(false)),
        &config(),
    );
    assert_eq!(out.events_total, 0);
    assert!(out.events_completed.is_empty());
    assert!(backend.log.is_empty());
}

#[test]
fn paste_dispatch_reports_one_event_not_application_text_success() {
    let text = "中🙂\n\tend";
    let mut backend = MockBackend::new(1, 1);
    let out = execute_plan(
        &compile_plan(&paste(text), 1),
        &mut backend,
        |p| (p[0], p[1]),
        &Arc::new(AtomicBool::new(false)),
        &config(),
    );
    assert_eq!(out.input_outcome, InputOutcome::Dispatched);
    assert_eq!(out.events_total, 1);
    assert_eq!(out.events_completed.len(), 1);
    assert_eq!(out.cleanup_outcome, CleanupOutcome::NotNeeded);
    assert_eq!(
        backend.log,
        vec![format!("inject:TextPaste {{ text: {text:?} }}")]
    );
}

#[test]
fn cancellation_before_paste_dispatches_nothing() {
    let mut backend = MockBackend::new(1, 1);
    let out = execute_plan(
        &compile_plan(&paste("not sent"), 1),
        &mut backend,
        |p| (p[0], p[1]),
        &Arc::new(AtomicBool::new(true)),
        &config(),
    );
    assert_eq!(out.input_outcome, InputOutcome::NotStarted);
    assert!(out.cancelled);
    assert!(out.events_completed.is_empty());
    assert_eq!(backend.inject_count(), 0);
}

#[test]
fn cancellation_during_preflight_cannot_start_paste() {
    let cancel = Arc::new(AtomicBool::new(false));
    let mut backend = MockBackend::new(1, 1);
    let out = execute_plan_guarded(
        &compile_plan(&paste("not sent"), 1),
        &mut backend,
        |p| (p[0], p[1]),
        &cancel,
        &config(),
        |_, _| {
            cancel.store(true, Ordering::SeqCst);
            Ok(())
        },
    );
    assert_eq!(out.input_outcome, InputOutcome::NotStarted);
    assert!(out.cancelled);
    assert_eq!(backend.inject_count(), 0);
}

#[test]
fn paste_failure_is_reported_without_retry_or_native_fallback() {
    let mut backend = MockBackend::new(1, 1);
    backend
        .inject_results
        .push_back(Err(err("paste_failed", "clipboard may have changed")));
    let out = execute_plan(
        &compile_plan(&paste("not retried"), 1),
        &mut backend,
        |p| (p[0], p[1]),
        &Arc::new(AtomicBool::new(false)),
        &config(),
    );
    assert_ne!(out.input_outcome, InputOutcome::Dispatched);
    assert!(out.events_completed.is_empty());
    let error = out.error.unwrap();
    assert_eq!(error.code, codes::INPUT_ERROR);
    assert!(error.message.contains("clipboard may have changed"));
    assert_eq!(backend.inject_count(), 1);
    assert_eq!(backend.release_count(), 1);
}

#[test]
fn schema_keeps_method_optional_and_discloses_clipboard_side_effects() {
    let tools = tool_definitions();
    let step = tools.iter().find(|t| t.name == "computer_step").unwrap();
    let action = step.input_schema["properties"]["action"]["oneOf"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["properties"]["kind"]["const"] == "text_input")
        .unwrap();
    assert_eq!(action["required"], json!(["kind", "text"]));
    assert_eq!(action["additionalProperties"], false);
    assert_eq!(
        action["properties"]["method"]["enum"],
        json!(["native", "paste"])
    );
    assert_eq!(action["properties"]["method"]["default"], "native");
    assert_eq!(action["properties"]["text"]["maxLength"], TEXT_MAX_CHARS);
    let description = action["properties"]["method"]["description"]
        .as_str()
        .unwrap();
    for required in [
        "Windows-only",
        "overwrite",
        "clipboard",
        "leaves",
        "restore",
        "fallback",
        "IME",
    ] {
        assert!(
            description.contains(required),
            "missing disclosure: {required}"
        );
    }
}
