//! No OS calls: real root DisplayProvider + library engine with memory frames.
use super::*;
use crate::backend::{Backend, Capture, InputEvent};
use rpa_windows_display::{DisplayError, RgbaFrame, SourcePermit};
use std::{cell::RefCell, rc::Rc};

pub struct Facts {
    pub displays: Vec<DisplayDescriptor>,
    pub events: Vec<InputEvent>,
    pub queries: usize,
    pub captures: usize,
    pub releases: usize,
    pub query_fail: bool,
    pub capture_fail: bool,
    pub release_fail: bool,
    pub change_after_events: Option<usize>,
    pub fail_after_events: Option<usize>,
    pub change_during_capture: bool,
    pub cancel_after_events: Option<(usize, std::sync::Arc<std::sync::atomic::AtomicBool>)>,
}
pub type Shared = Rc<RefCell<Facts>>;
pub fn facts() -> Shared {
    Rc::new(RefCell::new(Facts {
        displays: vec![
            DisplayDescriptor {
                id: "left".into(),
                is_primary: true,
                native_bounds: NativeRect {
                    x: -100,
                    y: 0,
                    width: 100,
                    height: 80,
                },
                capture_size: PixelSize {
                    width: 100,
                    height: 80,
                },
                scale: 1.0,
                rotation: Rotation::Deg0,
            },
            DisplayDescriptor {
                id: "right".into(),
                is_primary: false,
                native_bounds: NativeRect {
                    x: 40,
                    y: 30,
                    width: 60,
                    height: 120,
                },
                capture_size: PixelSize {
                    width: 120,
                    height: 240,
                },
                scale: 2.0,
                rotation: Rotation::Deg0,
            },
        ],
        events: vec![],
        queries: 0,
        captures: 0,
        releases: 0,
        query_fail: false,
        capture_fail: false,
        release_fail: false,
        change_after_events: None,
        fail_after_events: None,
        change_during_capture: false,
        cancel_after_events: None,
    }))
}
pub struct MemoryFrames(pub Shared);
impl FrameProvider for MemoryFrames {
    fn enumerate(&mut self) -> Result<Vec<DisplayDescriptor>, DisplayError> {
        let mut f = self.0.borrow_mut();
        f.queries += 1;
        if f.query_fail {
            return Err(DisplayError::Native {
                operation: "mock enumeration",
                code: 5,
            });
        }
        Ok(f.displays.clone())
    }
    fn capture(
        &mut self,
        display: &DisplayDescriptor,
        permit: SourcePermit,
    ) -> Result<RgbaFrame, DisplayError> {
        let mut f = self.0.borrow_mut();
        f.captures += 1;
        if f.capture_fail {
            return Err(DisplayError::Native {
                operation: "mock capture",
                code: 5,
            });
        }
        assert_eq!(permit.size(), display.capture_size);
        if f.change_during_capture {
            f.displays[0].scale += 0.25;
        }
        let color = if display.id == "left" {
            [230, 10, 20, 255]
        } else {
            [20, 40, 240, 255]
        };
        let pixels = color.repeat((permit.bytes() / 4) as usize);
        RgbaFrame::new(permit.size(), pixels)
    }
}
pub struct MemoryDesktop {
    pub display: DisplayProvider<MemoryFrames>,
    pub facts: Shared,
}
impl MemoryDesktop {
    pub fn new(facts: Shared) -> Self {
        Self {
            display: DisplayProvider::new(MemoryFrames(facts.clone())).unwrap(),
            facts,
        }
    }
}
impl Backend for MemoryDesktop {
    fn platform(&self) -> &'static str {
        "mock-multidisplay"
    }
    fn display_selections(&self) -> &'static [&'static str] {
        &["primary", "id", "desktop"]
    }
    fn display_snapshot(&mut self) -> Result<Option<TopologySnapshot>, BackendError> {
        self.display.snapshot().map(Some)
    }
    fn select_display(&mut self, s: &Selection) -> Result<Geometry, BackendError> {
        self.display.select(s)
    }
    fn capture_display(
        &mut self,
        b: CaptureBudget,
    ) -> Result<Option<DisplayCapture>, BackendError> {
        self.display.capture(b).map(Some)
    }
    fn geometry(&mut self) -> Result<Geometry, BackendError> {
        self.display.geometry()
    }
    fn capture(&mut self) -> Result<Capture, BackendError> {
        panic!("root multidisplay path must not fall back to legacy capture")
    }
    fn inject(&mut self, e: &InputEvent) -> Result<(), BackendError> {
        let mut f = self.facts.borrow_mut();
        f.events.push(e.clone());
        if let Some((at, cancel)) = &f.cancel_after_events {
            if *at == f.events.len() {
                cancel.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        }
        if f.change_after_events == Some(f.events.len()) {
            f.displays[1].scale += 0.25;
        }
        if f.fail_after_events == Some(f.events.len()) {
            f.query_fail = true;
        }
        Ok(())
    }
    fn release_all(&mut self) -> Result<(), BackendError> {
        let mut f = self.facts.borrow_mut();
        f.releases += 1;
        if f.release_fail {
            Err(BackendError::new("cleanup_error", "mock release failed"))
        } else {
            Ok(())
        }
    }
}
