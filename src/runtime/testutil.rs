//! A deterministic in-memory `Backend` for runtime tests. Records every
//! injected event and can be scripted to fail at chosen points. Never touches
//! a real desktop.

use std::collections::{BTreeSet, VecDeque};

pub(crate) mod race;

use crate::backend::{Backend, BackendError, Capture, Direction, Geometry, InputEvent};

#[derive(Debug, Clone, PartialEq)]
pub enum Recorded {
    Event(InputEvent),
    ReleaseAll,
    Geometry,
    Capture,
}

pub struct FakeBackend {
    display: crate::backend::display::single::SingleDisplay,
    pub geometry: Geometry,
    pub png: Vec<u8>,
    pub recorded: Vec<Recorded>,
    /// Fail the inject call when the count of prior inject calls equals the
    /// next queued value, with the queued error.
    pub fail_inject_at: VecDeque<(usize, BackendError)>,
    pub inject_calls: usize,
    pub fail_release_all: Option<BackendError>,
    pub fail_capture: Option<BackendError>,
    pub fail_geometry: Option<BackendError>,
    /// Geometry version returned by `geometry()`; may be mutated mid-test to
    /// simulate a display change.
    pub pending_version: Option<String>,
    /// Once set, capture fails with `capture_error` after this many captures
    /// have SUCCEEDED (0 = the very first capture already fails). Models
    /// "screenshot error after input was dispatched".
    pub fail_capture_after: Option<usize>,
    capture_calls: usize,
    /// Test-only successful key-press checkpoint, after validation/recording.
    pub after_key_press_gate: Option<std::sync::Arc<race::RaceGate>>,
    /// Simulated held-state authority, cleared only by successful releases.
    pub held_keys: BTreeSet<String>,
}

pub fn fake_png(width: u32, height: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(width, height, image::Rgba([3, 4, 5, 255]));
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
    buf.into_inner()
}

pub fn fake_geometry() -> Geometry {
    Geometry {
        surface_id: "primary".into(),
        input_origin: (0, 0),
        input_size: (1600, 900),
        version: "v1".into(),
    }
}

impl FakeBackend {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            geometry: fake_geometry(),
            display: crate::backend::display::single::SingleDisplay::new(
                rpa_display_topology::NativeUnit::PhysicalPixels,
            ),
            png: fake_png(width, height),
            recorded: Vec::new(),
            fail_inject_at: VecDeque::new(),
            inject_calls: 0,
            fail_release_all: None,
            fail_capture: None,
            fail_geometry: None,
            pending_version: None,
            fail_capture_after: None,
            capture_calls: 0,
            after_key_press_gate: None,
            held_keys: BTreeSet::new(),
        }
    }

    /// Override the geometry the fake backend reports (e.g. a non-zero
    /// input origin) without touching the captured image.
    pub fn set_geometry(&mut self, geometry: Geometry) {
        self.geometry = geometry;
    }

    pub fn events(&self) -> Vec<&InputEvent> {
        self.recorded
            .iter()
            .filter_map(|r| match r {
                Recorded::Event(e) => Some(e),
                _ => None,
            })
            .collect()
    }

    pub fn release_all_calls(&self) -> usize {
        self.recorded
            .iter()
            .filter(|r| matches!(r, Recorded::ReleaseAll))
            .count()
    }
}

impl Backend for FakeBackend {
    fn display_snapshot(
        &mut self,
    ) -> Result<Option<rpa_display_topology::TopologySnapshot>, BackendError> {
        let g = self.geometry()?;
        let size = crate::runtime::image::decode_png(&self.png)
            .map_err(|e| be("capture_error", &e.message))?;
        self.display
            .snapshot(
                &g,
                rpa_display_topology::PixelSize {
                    width: size.width,
                    height: size.height,
                },
            )
            .map(Some)
    }
    fn platform(&self) -> &'static str {
        "fake"
    }

    fn geometry(&mut self) -> Result<Geometry, BackendError> {
        self.recorded.push(Recorded::Geometry);
        if let Some(e) = &self.fail_geometry {
            return Err(e.clone());
        }
        if let Some(v) = self.pending_version.take() {
            self.geometry.version = v;
        }
        Ok(self.geometry.clone())
    }

    fn capture(&mut self) -> Result<Capture, BackendError> {
        self.recorded.push(Recorded::Capture);
        self.capture_calls += 1;
        if let Some(e) = &self.fail_capture {
            return Err(e.clone());
        }
        if let Some(after) = self.fail_capture_after {
            if self.capture_calls > after {
                return Err(be("capture_error", "screen went away after input"));
            }
        }
        // Report the honest dimensions of the PNG we return: capture
        // metadata must describe the actual bytes, like a real backend.
        let info = crate::runtime::image::decode_png(&self.png)
            .expect("fake backend png must stay decodable");
        Ok(Capture {
            png: self.png.clone(),
            width: info.width,
            height: info.height,
            geometry: self.geometry.clone(),
        })
    }

    fn inject(&mut self, event: &InputEvent) -> Result<(), BackendError> {
        if let Some((at, err)) = self.fail_inject_at.front() {
            if *at == self.inject_calls {
                self.inject_calls += 1;
                let err = err.clone();
                self.fail_inject_at.pop_front();
                return Err(err);
            }
        }
        self.inject_calls += 1;
        // Validate names like the real backend contract requires.
        match event {
            InputEvent::Key { key, direction: _ } => {
                crate::backend::keys::parse_key(key)?;
            }
            InputEvent::Button {
                button,
                direction: _,
                click_count,
            } => {
                crate::backend::keys::parse_button(button)?;
                if !rpa_native_input::is_valid_click_count(*click_count) {
                    return Err(be("invalid_button", "click_count must be in 1..=3"));
                }
            }
            InputEvent::Text { text } => {
                crate::backend::keys::validate_text(text)?;
            }
            _ => {}
        }
        self.recorded.push(Recorded::Event(event.clone()));
        if let InputEvent::Key { key, direction } = event {
            match direction {
                Direction::Press => {
                    self.held_keys.insert(key.clone());
                    if let Some(gate) = self.after_key_press_gate.take() {
                        gate.checkpoint().map_err(|e| be("test_gate_timeout", e))?;
                    }
                }
                Direction::Release => {
                    self.held_keys.remove(key);
                }
            }
        }
        Ok(())
    }

    fn release_all(&mut self) -> Result<(), BackendError> {
        self.recorded.push(Recorded::ReleaseAll);
        match &self.fail_release_all {
            Some(e) => Err(e.clone()),
            None => {
                self.held_keys.clear();
                Ok(())
            }
        }
    }
}

pub fn be(code: &str, message: &str) -> BackendError {
    BackendError {
        code: code.to_string(),
        message: message.to_string(),
    }
}

#[allow(dead_code)]
pub fn is_press(e: &InputEvent, name: &str) -> bool {
    matches!(
        e,
        InputEvent::Key { key, direction: Direction::Press } if key == name
    ) || matches!(
        e,
        InputEvent::Button {
            button,
            direction: Direction::Press,
            ..
        } if button == name
    )
}
