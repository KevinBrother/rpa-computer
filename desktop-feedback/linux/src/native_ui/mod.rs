//! Renderer policy and lifecycle, also consumed by unchanged contract tests.
#![allow(dead_code, unused_imports)]
mod lifecycle;
mod paint;
pub mod process_boundary;
mod startup_input;
mod transport;
pub use lifecycle::*;
pub use paint::*;
pub use transport::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiFailure {
    Native,
    InvalidWindows,
    IncompletePresentation,
    InputShapeUnavailable,
    Geometry,
    InvalidStyle,
    AlreadyStarted,
    Transport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Status,
    Stop,
    Ring,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputRegion {
    Empty,
    StopRectangle,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowPolicy {
    pub role: Role,
    pub input: InputRegion,
    pub override_redirect: bool,
    pub accepts_focus: bool,
    pub requests_activation: bool,
    pub grabs_keyboard_or_pointer: bool,
    pub uses_composite_overlay: bool,
}

pub fn window_policies() -> [WindowPolicy; 3] {
    [Role::Status, Role::Stop, Role::Ring].map(|role| WindowPolicy {
        role,
        input: if role == Role::Stop {
            InputRegion::StopRectangle
        } else {
            InputRegion::Empty
        },
        override_redirect: true,
        accepts_focus: false,
        requests_activation: false,
        grabs_keyboard_or_pointer: false,
        uses_composite_overlay: false,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisualPolicy {
    Argb,
    OpaqueShaped,
}
pub fn visual_policy(
    compositor: bool,
    argb_visual: bool,
    input_shapes: bool,
) -> Result<VisualPolicy, UiFailure> {
    if !input_shapes {
        return Err(UiFailure::InputShapeUnavailable);
    }
    Ok(if compositor && argb_visual {
        VisualPolicy::Argb
    } else {
        VisualPolicy::OpaqueShaped
    })
}

#[cfg(all(target_os = "linux", feature = "native-ui"))]
pub mod process;
#[cfg(all(target_os = "linux", feature = "native-ui"))]
mod x11;
