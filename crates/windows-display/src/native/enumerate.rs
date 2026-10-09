use super::{clear_error, hresult, last_error};
use crate::{identity::Identity, mode::ModeFacts, DisplayError};
use rpa_display_topology::{NativeRect, PixelSize};
use std::mem::size_of;
use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::{BOOL, LPARAM, RECT},
        Graphics::Gdi::*,
        UI::{
            Shell::GetScaleFactorForMonitor,
            WindowsAndMessaging::{EDD_GET_DEVICE_INTERFACE_NAME, MONITORINFOF_PRIMARY},
        },
    },
};

const MAX_MONITORS: usize = 64;
const MAX_DEVICES: u32 = 256;
#[derive(Debug)]
pub(super) struct Monitor {
    pub identity: Identity,
    pub facts: ModeFacts,
}
struct Handles {
    monitors: Vec<HMONITOR>,
    rejected: bool,
}
unsafe extern "system" fn callback(
    monitor: HMONITOR,
    _dc: HDC,
    _rect: *mut RECT,
    data: LPARAM,
) -> BOOL {
    // SAFETY: EnumDisplayMonitors synchronously calls this with the address of
    // Handles that remains alive and exclusively borrowed for the entire call.
    let handles = unsafe { &mut *(data.0 as *mut Handles) };
    if monitor.is_invalid() || handles.monitors.len() == MAX_MONITORS {
        handles.rejected = true;
        return BOOL(0);
    }
    // Capacity is reserved before entering FFI: no callback allocation/unwind.
    handles.monitors.push(monitor);
    BOOL(1)
}
pub(super) fn monitors() -> Result<Vec<Monitor>, DisplayError> {
    let mut handles = Handles {
        monitors: Vec::with_capacity(MAX_MONITORS),
        rejected: false,
    };
    // SAFETY: Valid synchronous callback/context, no DC or clipping rectangle.
    clear_error();
    let success = unsafe {
        EnumDisplayMonitors(
            HDC::default(),
            None,
            Some(callback),
            LPARAM(&mut handles as *mut Handles as isize),
        )
    }
    .as_bool();
    if handles.rejected {
        return Err(DisplayError::EnumerationLimit);
    }
    if !success {
        return Err(last_error("EnumDisplayMonitors"));
    }
    if handles.monitors.is_empty() {
        return Err(rpa_display_topology::Error::EmptyTopology.into());
    }
    let adapters = adapters()?;
    handles
        .monitors
        .into_iter()
        .map(|handle| read_monitor(handle, &adapters))
        .collect()
}
fn read_monitor(handle: HMONITOR, adapters: &[DISPLAY_DEVICEW]) -> Result<Monitor, DisplayError> {
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
    // SAFETY: MONITORINFOEXW begins with MONITORINFO; cbSize requests the EXW tail.
    clear_error();
    if !unsafe { GetMonitorInfoW(handle, &mut info.monitorInfo) }.as_bool() {
        return Err(last_error("GetMonitorInfoW"));
    }
    let device = text(&info.szDevice)?;
    let adapter = adapters
        .iter()
        .find(|a| text(&a.DeviceName).as_ref() == Ok(&device))
        .ok_or(DisplayError::UnsupportedDisplay {
            reason: "monitor has no active display adapter",
        })?;
    if adapter.StateFlags & DISPLAY_DEVICE_MIRRORING_DRIVER != 0 {
        return Err(DisplayError::UnsupportedDisplay {
            reason: "mirroring pseudo-monitor is unsupported",
        });
    }
    let device_w = wide(&device);
    let mut mode = DEVMODEW {
        dmSize: size_of::<DEVMODEW>() as u16,
        ..Default::default()
    };
    // SAFETY: NUL-terminated device name and initialized complete DEVMODEW.
    clear_error();
    if !unsafe {
        EnumDisplaySettingsExW(
            PCWSTR(device_w.as_ptr()),
            ENUM_CURRENT_SETTINGS,
            &mut mode,
            ENUM_DISPLAY_SETTINGS_FLAGS(0),
        )
    }
    .as_bool()
    {
        return Err(last_error("EnumDisplaySettingsExW"));
    }
    let required = DM_POSITION | DM_PELSWIDTH | DM_PELSHEIGHT | DM_DISPLAYORIENTATION;
    if (mode.dmFields.0 & required.0) != required.0 {
        return Err(DisplayError::UnsupportedDisplay {
            reason: "DEVMODE missing physical position, size or rotation",
        });
    }
    // SAFETY: Required DM_POSITION/DM_DISPLAYORIENTATION flags select the display
    // union member, not the printer DEVMODE member.
    let display_mode = unsafe { mode.Anonymous1.Anonymous2 };
    let bounds = info.monitorInfo.rcMonitor;
    let width = u32::try_from(i64::from(bounds.right) - i64::from(bounds.left)).map_err(|_| {
        DisplayError::UnsupportedDisplay {
            reason: "invalid monitor horizontal bounds",
        }
    })?;
    let height = u32::try_from(i64::from(bounds.bottom) - i64::from(bounds.top)).map_err(|_| {
        DisplayError::UnsupportedDisplay {
            reason: "invalid monitor vertical bounds",
        }
    })?;
    // SAFETY: Current real HMONITOR, HRESULT is checked; never use the API's
    // documented default output on failure as an invented scale=1 fallback.
    let scale = unsafe { GetScaleFactorForMonitor(handle) }
        .map_err(|e| hresult("GetScaleFactorForMonitor", e))?;
    let interfaces = interfaces(&device_w)?;
    Ok(Monitor {
        identity: Identity {
            handle: handle.0 as usize,
            device,
            adapter: text(&adapter.DeviceID)?,
            interfaces,
        },
        facts: ModeFacts {
            bounds: NativeRect {
                x: bounds.left,
                y: bounds.top,
                width,
                height,
            },
            mode_origin: (display_mode.dmPosition.x, display_mode.dmPosition.y),
            mode_size: PixelSize {
                width: mode.dmPelsWidth,
                height: mode.dmPelsHeight,
            },
            orientation: display_mode.dmDisplayOrientation.0,
            scale_percent: scale.0,
            primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
        },
    })
}
fn adapters() -> Result<Vec<DISPLAY_DEVICEW>, DisplayError> {
    let mut found = Vec::new();
    for index in 0..MAX_DEVICES {
        let mut device = DISPLAY_DEVICEW {
            cb: size_of::<DISPLAY_DEVICEW>() as u32,
            ..Default::default()
        };
        // SAFETY: NULL selects adapter enumeration. FALSE is documented end-of-list.
        if !unsafe { EnumDisplayDevicesW(PCWSTR::null(), index, &mut device, 0) }.as_bool() {
            return Ok(found);
        }
        if device.StateFlags & DISPLAY_DEVICE_ATTACHED_TO_DESKTOP != 0 {
            found.push(device);
        }
    }
    Err(DisplayError::EnumerationLimit)
}
fn interfaces(device: &[u16]) -> Result<Vec<String>, DisplayError> {
    let mut found = Vec::new();
    for index in 0..MAX_DEVICES {
        let mut monitor = DISPLAY_DEVICEW {
            cb: size_of::<DISPLAY_DEVICEW>() as u32,
            ..Default::default()
        };
        // SAFETY: Device is terminated; interface-name flag requests stable OS
        // identity data, not a generated hash or user-visible display label.
        if !unsafe {
            EnumDisplayDevicesW(
                PCWSTR(device.as_ptr()),
                index,
                &mut monitor,
                EDD_GET_DEVICE_INTERFACE_NAME,
            )
        }
        .as_bool()
        {
            if found.is_empty() {
                return Err(DisplayError::UnsupportedDisplay {
                    reason: "no active monitor interface identity",
                });
            }
            found.sort();
            if found.windows(2).any(|p| p[0] == p[1]) {
                return Err(DisplayError::DuplicateNativeIdentity);
            }
            return Ok(found);
        }
        if monitor.StateFlags & DISPLAY_DEVICE_ACTIVE != 0 {
            found.push(text(&monitor.DeviceID)?);
        }
    }
    Err(DisplayError::EnumerationLimit)
}
fn text(value: &[u16]) -> Result<String, DisplayError> {
    let end = value
        .iter()
        .position(|c| *c == 0)
        .ok_or(DisplayError::UnsupportedDisplay {
            reason: "unterminated OS device string",
        })?;
    if end == 0 {
        return Err(DisplayError::UnsupportedDisplay {
            reason: "empty OS device identity",
        });
    }
    String::from_utf16(&value[..end]).map_err(|_| DisplayError::UnsupportedDisplay {
        reason: "invalid OS device UTF-16",
    })
}
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
