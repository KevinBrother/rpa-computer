//! All Win32 unsafe calls are localized to this directory. No hand-written ABI.
mod capture;
mod enumerate;

use crate::{identity::IdRegistry, DisplayError, FrameProvider, RgbaFrame, SourcePermit};
use rpa_display_topology::DisplayDescriptor;
use std::{marker::PhantomData, rc::Rc};
use windows::Win32::{
    Foundation::HANDLE,
    UI::HiDpi::{
        AreDpiAwarenessContextsEqual, GetDpiAwarenessContextForProcess,
        GetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    },
};

/// Verify BOTH current process and thread are PMv2. Does not set process DPI or
/// install a thread override. Callers must prepare/manifest awareness before attach.
pub fn verify_pmv2() -> Result<(), DisplayError> {
    // SAFETY: NULL process handle means current process per documented API.
    // These queries borrow no memory and return value-like awareness contexts.
    clear_error();
    let process = unsafe { GetDpiAwarenessContextForProcess(HANDLE::default()) };
    if process.0.is_null() {
        return Err(last_error("GetDpiAwarenessContextForProcess"));
    }
    clear_error();
    // SAFETY: Query only, no thread override or resource ownership transfer.
    let thread = unsafe { GetThreadDpiAwarenessContext() };
    if thread.0.is_null() {
        return Err(last_error("GetThreadDpiAwarenessContext"));
    }
    // SAFETY: Contexts returned above and the official PMv2 constant are valid.
    let (process_pmv2, thread_pmv2) = unsafe {
        (
            AreDpiAwarenessContextsEqual(process, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)
                .as_bool(),
            AreDpiAwarenessContextsEqual(thread, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)
                .as_bool(),
        )
    };
    crate::dpi::require_contexts(process_pmv2, thread_pmv2)
}

/// Real GDI provider. !Send + !Sync: attach/use/drop on the same prepared thread.
/// No import/constructor global DPI mutation; no cached screenshot fallback.
#[derive(Debug)]
pub struct GdiProvider {
    registry: IdRegistry,
    _owning_thread: PhantomData<Rc<()>>,
}
impl GdiProvider {
    /// Attach after verifying PMv2 on process AND calling thread; no enumeration
    /// or screenshot occurs until the explicit provider operations.
    pub fn attach() -> Result<Self, DisplayError> {
        verify_pmv2()?;
        Ok(Self {
            registry: IdRegistry::default(),
            _owning_thread: PhantomData,
        })
    }
}
impl FrameProvider for GdiProvider {
    fn enumerate(&mut self) -> Result<Vec<DisplayDescriptor>, DisplayError> {
        let result = (|| {
            verify_pmv2()?;
            let monitors = enumerate::monitors()?;
            // Validate every OS fact BEFORE committing ID allocation.
            for monitor in &monitors {
                monitor.facts.descriptor("validation".into())?;
            }
            let identities = monitors
                .iter()
                .map(|m| m.identity.clone())
                .collect::<Vec<_>>();
            let ids = self.registry.reconcile(&identities)?;
            let descriptors = monitors
                .iter()
                .zip(ids)
                .map(|(m, id)| m.facts.descriptor(id))
                .collect::<Result<Vec<_>, _>>()?;
            let primaries: Vec<_> = descriptors
                .iter()
                .filter(|d| d.is_primary)
                .map(|d| d.id.clone())
                .collect();
            match primaries.len() {
                0 => return Err(rpa_display_topology::Error::NoPrimaryDisplay.into()),
                1 => {}
                _ => {
                    return Err(rpa_display_topology::Error::MultiplePrimaryDisplays {
                        ids: primaries,
                    }
                    .into())
                }
            }
            verify_pmv2()?;
            Ok(descriptors)
        })();
        if result.is_err() {
            self.registry.invalidate();
        }
        result
    }

    fn capture(
        &mut self,
        display: &DisplayDescriptor,
        permit: SourcePermit,
    ) -> Result<RgbaFrame, DisplayError> {
        // Fresh enumeration revalidates HMONITOR + exact adapter/interface identity
        // before using global physical bounds. Old HMONITOR is NEVER blindly reused.
        let current = self.enumerate()?;
        let actual = current.iter().find(|d| d.id == display.id).ok_or_else(|| {
            DisplayError::SourceChanged {
                id: display.id.clone(),
            }
        })?;
        if actual != display {
            return Err(DisplayError::SourceChanged {
                id: display.id.clone(),
            });
        }
        permit.require(display.capture_size)?;
        let frame = capture::capture(display, permit)?;
        verify_pmv2()?;
        Ok(frame)
    }
}
fn last_error(operation: &'static str) -> DisplayError {
    // SAFETY: Thread-local last-error query, immediately after the failing call.
    DisplayError::Native {
        operation,
        code: unsafe { windows::Win32::Foundation::GetLastError().0 },
    }
}
fn hresult(operation: &'static str, error: windows::core::Error) -> DisplayError {
    DisplayError::Native {
        operation,
        code: error.code().0 as u32,
    }
}

fn clear_error() {
    // SAFETY: Set only this thread's error slot before a Win32 operation whose
    // documented failure may not carry extended detail; zero means unspecified.
    unsafe {
        windows::Win32::Foundation::SetLastError(windows::Win32::Foundation::WIN32_ERROR(0));
    }
}
