mod encode;

use crate::flight::Flight;
use crate::selection::{select_targets, ApplicationId, DisplayId, MAX_APPLICATIONS, MAX_DISPLAYS};
use crate::{
    classify_native_error, CaptureError, CaptureRequest, CapturedImage, ErrorKind,
    NativeErrorDomain, Result, Stage,
};
use block2::RcBlock;
use objc2::rc::{autoreleasepool, Retained};
use objc2::AnyThread;
use objc2_core_graphics::{CGImage, CGPreflightScreenCaptureAccess};
use objc2_foundation::{NSArray, NSError, NSOperatingSystemVersion, NSProcessInfo, NSThread};
use objc2_screen_capture_kit::{
    SCContentFilter, SCRunningApplication, SCScreenshotManager, SCShareableContent,
    SCStreamConfiguration, SCStreamErrorDomain, SCWindow,
};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;
use std::time::Instant;

pub(crate) fn is_supported() -> bool {
    // Foundation only; no reference to a post-14 class/selector before this guard.
    NSProcessInfo::processInfo().isOperatingSystemAtLeastVersion(NSOperatingSystemVersion {
        majorVersion: 14,
        minorVersion: 0,
        patchVersion: 0,
    })
}

pub(crate) fn preflight() -> Result<()> {
    if !is_supported() {
        return Err(CaptureError::new(
            ErrorKind::Unsupported,
            Stage::Availability,
        ));
    }
    if NSThread::isMainThread_class() {
        return Err(CaptureError::new(
            ErrorKind::InvalidRequest,
            Stage::Preflight,
        ));
    }
    // Does not call CGRequestScreenCaptureAccess and does not change TCC.
    if !CGPreflightScreenCaptureAccess() {
        return Err(CaptureError::new(
            ErrorKind::PermissionDenied,
            Stage::Preflight,
        ));
    }
    Ok(())
}

/// The only native dispatch path. Escaping blocks own Arc state, never stack
/// references, borrowed channels or the CaptureClient. No per-call worker thread.
pub(crate) fn start(request: CaptureRequest, flight: Arc<Flight<CapturedImage>>) {
    if !flight.may_continue_at(Instant::now()) {
        flight.complete_at(
            Err(CaptureError::new(ErrorKind::Timeout, Stage::Discovery)),
            Instant::now(),
        );
        return;
    }
    let callback_flight = Arc::clone(&flight);
    let discovery = RcBlock::new(
        move |content: *mut SCShareableContent, error: *mut NSError| {
            if !callback_flight.claim_discovery() {
                return;
            }
            let dispatch = catch_unwind(AssertUnwindSafe(|| {
                autoreleasepool(|_| {
                    if !callback_flight.may_continue_at(Instant::now()) {
                        return Err(CaptureError::new(ErrorKind::Timeout, Stage::Discovery));
                    }
                    // SAFETY: Apple passes nullable borrowed objects alive for this
                    // callback. We read them only inside the callback/autorelease pool.
                    if let Some(error) = unsafe { error.as_ref() } {
                        return Err(native_error(error, Stage::Discovery));
                    }
                    let content = unsafe { content.as_ref() }.ok_or(CaptureError::new(
                        ErrorKind::CaptureFailed,
                        Stage::Discovery,
                    ))?;
                    let resources = prepare(content, request)?;
                    dispatch_image(resources, Arc::clone(&callback_flight));
                    Ok(())
                })
            }));
            match dispatch {
                Ok(Ok(())) => {} // Screenshot callback is now responsible for completion.
                Ok(Err(error)) => callback_flight.complete_at(Err(error), Instant::now()),
                Err(_) => callback_flight.complete_at(
                    Err(CaptureError::new(
                        ErrorKind::CaptureFailed,
                        Stage::Discovery,
                    )),
                    Instant::now(),
                ),
            }
        },
    );
    // SAFETY: macOS14 and preflight checked before start. RcBlock owns heap Arc
    // state and Apple retains/copies the asynchronous completion block. It can
    // arrive after start/capture/client have returned or been dropped.
    autoreleasepool(|_| unsafe {
        SCShareableContent::getShareableContentExcludingDesktopWindows_onScreenWindowsOnly_completionHandler(false, false, &discovery);
    });
}

/// These objects remain retained by the screenshot block until its native owner
/// releases that block. No configuration is mutated after dispatch.
struct Resources {
    filter: Retained<SCContentFilter>,
    configuration: Retained<SCStreamConfiguration>,
    // Pin original content/arrays for the lifetime of all borrowed members too.
    _content: Retained<SCShareableContent>,
    _excluded: Retained<NSArray<SCRunningApplication>>,
    _exceptions: Retained<NSArray<SCWindow>>,
}

fn prepare(content: &SCShareableContent, request: CaptureRequest) -> Result<Resources> {
    // SAFETY: content is valid for this callback; getters return retained arrays.
    let displays = unsafe { content.displays() };
    let applications = unsafe { content.applications() };
    if displays.count() > MAX_DISPLAYS || applications.count() > MAX_APPLICATIONS {
        return Err(CaptureError::new(
            ErrorKind::CaptureFailed,
            Stage::Discovery,
        ));
    }
    let display_ids: Vec<_> = displays
        .iter()
        .map(|display| DisplayId(unsafe { display.displayID() }))
        .collect();
    let app_ids: Vec<_> = applications
        .iter()
        .map(|app| {
            // Nonpositive PID cannot match a valid request; no app/title/string getter.
            let pid = unsafe { app.processID() };
            ApplicationId(u32::try_from(pid).unwrap_or(0))
        })
        .collect();
    let selection = select_targets(
        request.display_id,
        request.excluded_process_id,
        &display_ids,
        &app_ids,
    )?;
    let display = displays.objectAtIndex(selection.display_index);
    let application = applications.objectAtIndex(selection.application_index);
    let excluded: Retained<NSArray<SCRunningApplication>> = NSArray::from_slice(&[&application]);
    let exceptions: Retained<NSArray<SCWindow>> = NSArray::from_slice(&[]);
    // SAFETY: official generated initializer; exactly one selected PID's app,
    // selected native display, empty exceptions => never re-include renderer.
    let filter = unsafe {
        SCContentFilter::initWithDisplay_excludingApplications_exceptingWindows(
            SCContentFilter::alloc(),
            &display,
            &excluded,
            &exceptions,
        )
    };
    let configuration = unsafe { SCStreamConfiguration::new() };
    // All configuration setters used here exist before macOS14 (no 26-only HDR
    // API). Full display, no crop, no sample buffers, no audio or global cursor.
    unsafe {
        configuration.setWidth(request.width as usize);
        configuration.setHeight(request.height as usize);
        configuration.setScalesToFit(true);
        configuration.setShowsCursor(false);
        configuration.setCapturesAudio(false);
    }
    // SAFETY: retain the borrowed callback object before the callback returns.
    let owned = unsafe {
        Retained::retain(content as *const SCShareableContent as *mut SCShareableContent)
    }
    .ok_or(CaptureError::new(
        ErrorKind::CaptureFailed,
        Stage::Discovery,
    ))?;
    Ok(Resources {
        filter,
        configuration,
        _content: owned,
        _excluded: excluded,
        _exceptions: exceptions,
    })
}

fn dispatch_image(resources: Resources, flight: Arc<Flight<CapturedImage>>) {
    if !flight.may_continue_at(Instant::now()) {
        flight.complete_at(
            Err(CaptureError::new(ErrorKind::Timeout, Stage::Capture)),
            Instant::now(),
        );
        return;
    }
    // RcBlock accepts Fn; duplicate completion must not launch duplicate encoding.
    // Borrow resources for dispatch while the block owns a cloned, retained set.
    let filter = resources.filter.clone();
    let configuration = resources.configuration.clone();
    let image_callback = RcBlock::new(move |image: *mut CGImage, error: *mut NSError| {
        let _pins = &resources;
        if !flight.claim_image() {
            return;
        }
        let encoded = catch_unwind(AssertUnwindSafe(|| {
            autoreleasepool(|_| {
                if !flight.may_continue_at(Instant::now()) {
                    return Err(CaptureError::new(ErrorKind::Timeout, Stage::Capture));
                }
                // SAFETY: image/error are valid borrowed callback parameters. We
                // encode synchronously here, before Apple releases the CGImage.
                if let Some(error) = unsafe { error.as_ref() } {
                    return Err(native_error(error, Stage::Capture));
                }
                let image = unsafe { image.as_ref() }
                    .ok_or(CaptureError::new(ErrorKind::CaptureFailed, Stage::Capture))?;
                encode::png(image, &flight)
            })
        }))
        .unwrap_or_else(|_| Err(CaptureError::new(ErrorKind::EncodingFailed, Stage::Encode)));
        // Pool drained and native encoder resources released before notifying the
        // waiter. Deadline checked again inside complete; a late image is dropped.
        flight.complete_at(encoded, Instant::now());
    });
    // SAFETY: real filter/configuration are retained until completion. The heap
    // block owns them; it captures no stack objects. Official API is macOS14+.
    unsafe {
        SCScreenshotManager::captureImageWithFilter_configuration_completionHandler(
            &filter,
            &configuration,
            Some(&image_callback),
        );
    }
}

fn native_error(error: &NSError, stage: Stage) -> CaptureError {
    // Do not use localizedDescription / userInfo / bundle names / window titles.
    let domain = if &*error.domain() == unsafe { SCStreamErrorDomain } {
        NativeErrorDomain::ScreenCaptureKit
    } else {
        NativeErrorDomain::Other
    };
    let code = error.code() as i64;
    CaptureError::with_native(classify_native_error(domain, code), stage, domain, code)
}
