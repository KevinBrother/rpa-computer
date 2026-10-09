//! Runtime display contracts; no OS calls outside the Backend boundary.
use crate::backend::display::metadata;
use crate::backend::{Backend, BackendError};
use crate::runtime::error::{codes, ToolError};
use rpa_display_topology::{Generation, Selection, TopologySnapshot};
use serde_json::{json, Value};

pub fn selection(args: &Value) -> Result<Selection, ToolError> {
    let fail = || {
        ToolError::new(codes::INVALID_ARGUMENTS,"open expects only max_width/max_height and display:{kind:primary|id|desktop}; id requires a nonempty ID")
    };
    let object = args.as_object().ok_or_else(fail)?;
    if object
        .keys()
        .any(|k| !matches!(k.as_str(), "display" | "max_width" | "max_height"))
    {
        return Err(fail());
    }
    let Some(value) = object.get("display") else {
        return Ok(Selection::Primary);
    };
    let d = value.as_object().ok_or_else(fail)?;
    match d.get("kind").and_then(Value::as_str) {
        Some("primary") if d.len() == 1 => Ok(Selection::Primary),
        Some("desktop") if d.len() == 1 => Ok(Selection::Desktop),
        Some("id") if d.len() == 2 => d
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty() && id.len() <= 128 && *id != "desktop")
            .map(|id| Selection::Display(id.to_owned()))
            .ok_or_else(fail),
        _ => Err(fail()),
    }
}
pub fn describe(backend: &mut dyn Backend, mut data: Value) -> Value {
    let result = (|| -> Result<Option<TopologySnapshot>, BackendError> {
        let s = backend.display_snapshot()?;
        if let Some(s) = &s {
            metadata::validate(s)?;
        } else {
            backend.geometry()?;
        }
        Ok(s)
    })();
    data["available"] = json!(result.is_ok());
    match result {
        Ok(Some(s)) => {
            let topo = metadata::topology(&s);
            for (k, v) in topo.as_object().unwrap() {
                data[k] = v.clone();
            }
            data["display_topology"] = topo;
            data["display_selections"] = json!(backend.display_selections());
        }
        Ok(None) => {
            data["display_topology"] = Value::Null;
            data["display_selections"] = json!(["primary"]);
            data["topology_note"] = json!(
                "backend has explicit primary-only legacy geometry; no complete topology authority"
            );
        }
        Err(e) => {
            data["display_topology"] = Value::Null;
            data["preflight_error"] = json!({"code":e.code,"message":e.message});
        }
    }
    data
}
pub fn check(
    backend: &mut dyn Backend,
    bound: Option<Generation>,
) -> Result<Option<TopologySnapshot>, ToolError> {
    let latest = backend.display_snapshot().map_err(ToolError::from)?;
    if let Some(snapshot) = &latest {
        metadata::validate(snapshot).map_err(ToolError::from)?;
    }
    if let Some(expected) = bound {
        if latest.as_ref().map(|s| s.generation()) != Some(expected) {
            return Err(ToolError::new(
                codes::GEOMETRY_CHANGED,
                "complete topology generation changed or disappeared; close and reopen",
            ));
        }
    }
    Ok(latest)
}
