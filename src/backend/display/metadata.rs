use super::{error, BackendError, Geometry};
use rpa_display_topology::*;
use serde_json::{json, Value};

// Use the library's lossless, bounded wire token; Debug is not a protocol codec.
pub fn generation(g: Generation) -> String {
    g.to_token()
}
pub fn unit(u: NativeUnit) -> &'static str {
    match u {
        NativeUnit::Points => "points",
        NativeUnit::PhysicalPixels => "physical_pixels",
    }
}
pub fn rect(r: NativeRect) -> Value {
    json!({"x":r.x,"y":r.y,"width":r.width,"height":r.height})
}
pub fn validate(s: &TopologySnapshot) -> Result<(), BackendError> {
    if s.displays().len() > 64
        || s.displays()
            .iter()
            .any(|d| d.id.len() > 128 || d.id == "desktop")
    {
        return Err(BackendError::new(
            "display_error",
            "display count/id limit or reserved desktop ID",
        ));
    }
    Ok(())
}
pub fn topology(s: &TopologySnapshot) -> Value {
    let displays: Vec<_> = s
        .displays()
        .iter()
        .map(|d| {
            json!({
                "id":d.id,"is_primary":d.is_primary,"native_bounds":rect(d.native_bounds),
                "capture_size":{"width":d.capture_size.width,"height":d.capture_size.height},
                "scale":d.scale,"rotation":format!("{:?}",d.rotation)
            })
        })
        .collect();
    json!({"displays":displays,"native_unit":unit(s.unit()),"topology_generation":generation(s.generation())})
}
pub fn observation(m: &ObservationMapping) -> Value {
    let regions:Vec<_>=m.regions().iter().map(|r|json!({"display_id":r.display_id,
        "native_bounds":rect(r.native_bounds),"image_rect":{"x":r.image_rect.x,"y":r.image_rect.y,"width":r.image_rect.width,"height":r.image_rect.height}})).collect();
    json!({"native_unit":unit(m.unit()),"topology_generation":generation(m.generation()),
        "selected_display_ids":m.regions().iter().map(|r|&r.display_id).collect::<Vec<_>>(),"mapping_regions":regions})
}
pub fn geometry(s: &TopologySnapshot, selection: &Selection) -> Result<Geometry, BackendError> {
    validate(s)?;
    // Native bounding box from selected facts; not from resized image ratios.
    let selected: Vec<_> = s
        .displays()
        .iter()
        .filter(|d| match selection {
            Selection::Primary => d.is_primary,
            Selection::Display(id) => &d.id == id,
            Selection::Desktop => true,
        })
        .collect();
    if selected.is_empty() {
        return Err(BackendError::new(
            "display_not_found",
            "selected display is absent",
        ));
    }
    let left = selected
        .iter()
        .map(|d| i64::from(d.native_bounds.x))
        .min()
        .unwrap();
    let top = selected
        .iter()
        .map(|d| i64::from(d.native_bounds.y))
        .min()
        .unwrap();
    let right = selected
        .iter()
        .map(|d| i64::from(d.native_bounds.x) + i64::from(d.native_bounds.width))
        .max()
        .unwrap();
    let bottom = selected
        .iter()
        .map(|d| i64::from(d.native_bounds.y) + i64::from(d.native_bounds.height))
        .max()
        .unwrap();
    Ok(Geometry {
        surface_id: if *selection == Selection::Desktop {
            "desktop".into()
        } else {
            selected[0].id.clone()
        },
        input_origin: (left as i32, top as i32),
        input_size: (
            u32::try_from(right - left).map_err(error)?,
            u32::try_from(bottom - top).map_err(error)?,
        ),
        version: generation(s.generation()),
    })
}

#[cfg(test)]
mod token_tests;
