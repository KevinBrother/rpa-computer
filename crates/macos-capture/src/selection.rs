use crate::{CaptureError, ErrorKind, Result, Stage};

pub(crate) const MAX_DISPLAYS: usize = 256;
pub(crate) const MAX_APPLICATIONS: usize = 16_384;

#[derive(Debug, Clone, Copy)]
pub(crate) struct DisplayId(pub u32);
#[derive(Debug, Clone, Copy)]
pub(crate) struct ApplicationId(pub u32);
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SelectedTargets {
    pub display_index: usize,
    pub application_index: usize,
}

/// This seam is used directly by the native discovery callback. Neither names
/// nor window titles are collected or matched. Ambiguity is fail-closed.
pub(crate) fn select_targets(
    display: u32,
    pid: u32,
    displays: &[DisplayId],
    applications: &[ApplicationId],
) -> Result<SelectedTargets> {
    if displays.len() > MAX_DISPLAYS || applications.len() > MAX_APPLICATIONS {
        return Err(CaptureError::new(
            ErrorKind::CaptureFailed,
            Stage::Discovery,
        ));
    }
    let mut display_matches = displays.iter().enumerate().filter(|(_, d)| d.0 == display);
    let display_index = display_matches
        .next()
        .map(|(index, _)| index)
        .ok_or(CaptureError::new(ErrorKind::NoDisplay, Stage::Discovery))?;
    if display_matches.next().is_some() {
        return Err(CaptureError::new(
            ErrorKind::CaptureFailed,
            Stage::Discovery,
        ));
    }
    let mut app_matches = applications.iter().enumerate().filter(|(_, a)| a.0 == pid);
    let application_index = app_matches
        .next()
        .map(|(index, _)| index)
        .ok_or(CaptureError::new(
            ErrorKind::ExcludedProcessMissing,
            Stage::Discovery,
        ))?;
    if app_matches.next().is_some() {
        return Err(CaptureError::new(
            ErrorKind::CaptureFailed,
            Stage::Discovery,
        ));
    }
    Ok(SelectedTargets {
        display_index,
        application_index,
    })
}
