use rpa_display_topology::*;
use rpa_windows_display::*;
use std::cell::Cell;
use std::collections::VecDeque;
use std::rc::Rc;

struct Provider {
    states: VecDeque<Result<Vec<DisplayDescriptor>, DisplayError>>,
    frames: VecDeque<Result<RgbaFrame, DisplayError>>,
    captures: Rc<Cell<usize>>,
}
impl FrameProvider for Provider {
    fn enumerate(&mut self) -> Result<Vec<DisplayDescriptor>, DisplayError> {
        if self.states.len() > 1 {
            self.states.pop_front().unwrap()
        } else {
            self.states.front().unwrap().clone()
        }
    }
    fn capture(
        &mut self,
        _display: &DisplayDescriptor,
        _permit: SourcePermit,
    ) -> Result<RgbaFrame, DisplayError> {
        self.captures.set(self.captures.get() + 1);
        self.frames.pop_front().unwrap()
    }
}
fn d(id: &str, primary: bool, x: i32, y: i32, w: u32, h: u32, scale: f64) -> DisplayDescriptor {
    DisplayDescriptor {
        id: id.into(),
        is_primary: primary,
        native_bounds: NativeRect {
            x,
            y,
            width: w,
            height: h,
        },
        capture_size: PixelSize {
            width: w,
            height: h,
        },
        scale,
        rotation: Rotation::Deg0,
    }
}
fn frame(w: u32, h: u32, pixels: Vec<u8>) -> RgbaFrame {
    RgbaFrame::new(
        PixelSize {
            width: w,
            height: h,
        },
        pixels,
    )
    .unwrap()
}
fn solid(w: u32, h: u32, pixel: [u8; 4]) -> RgbaFrame {
    frame(w, h, pixel.repeat((w * h) as usize))
}
fn backend(
    states: Vec<Vec<DisplayDescriptor>>,
    frames: Vec<Result<RgbaFrame, DisplayError>>,
    memory: MemoryBudget,
) -> (DisplayBackend<Provider>, Rc<Cell<usize>>) {
    let captures = Rc::new(Cell::new(0));
    let provider = Provider {
        states: states.into_iter().map(Ok).collect(),
        frames: frames.into(),
        captures: captures.clone(),
    };
    (DisplayBackend::new(provider, memory).unwrap(), captures)
}
fn memory() -> MemoryBudget {
    MemoryBudget::new(1_000_000, 10_000_000).unwrap()
}
fn budget(w: u64, h: u64, pixels: u64) -> CaptureBudget {
    CaptureBudget::new(w, h, pixels).unwrap()
}

#[test]
fn real_composition_contract_negative_origins_mixed_dpi_gaps_and_png() {
    let mut left = d("left", false, -3, -1, 2, 2, 1.5);
    left.rotation = Rotation::Deg90;
    let right = d("right", true, 0, -1, 2, 2, 2.0);
    let (mut b, calls) = backend(
        vec![vec![right, left]],
        vec![
            Ok(solid(2, 2, [255, 0, 0, 255])),
            Ok(solid(2, 2, [0, 255, 0, 255])),
        ],
        memory(),
    );
    let out = b.observe(Selection::Desktop, budget(5, 2, 10)).unwrap();
    assert_eq!(calls.get(), 2);
    assert_eq!(out.topology.unit(), NativeUnit::PhysicalPixels);
    assert_eq!(out.topology.displays()[0].rotation, Rotation::Deg90);
    assert_eq!(out.topology.displays()[0].scale, 1.5);
    assert_eq!(
        out.rgba.size(),
        PixelSize {
            width: 5,
            height: 2
        }
    );
    let row = vec![
        255, 0, 0, 255, 255, 0, 0, 255, 0, 0, 0, 255, 0, 255, 0, 255, 0, 255, 0, 255,
    ];
    assert_eq!(out.rgba.pixels(), [row.clone(), row].concat());
    let decoded = image::load_from_memory_with_format(&out.png, image::ImageFormat::Png)
        .unwrap()
        .into_rgba8();
    assert_eq!(decoded.dimensions(), (5, 2));
    assert_eq!(decoded.as_raw(), out.rgba.pixels());
    assert!(matches!(
        out.mapping
            .map_image(ImagePoint { x: 2.0, y: 0.0 }, out.topology.generation()),
        Err(Error::ImageGapOrPadding { .. })
    ));
    assert_eq!(
        out.mapping
            .map_image(ImagePoint { x: 0.0, y: 0.0 }, out.topology.generation())
            .unwrap(),
        NativePoint { x: -3, y: -1 }
    );
}

#[test]
fn selected_source_absence_never_captures_primary() {
    let (mut b, calls) = backend(
        vec![vec![d("main", true, 0, 0, 2, 2, 1.0)]],
        vec![],
        memory(),
    );
    assert!(matches!(
        b.observe(Selection::Display("missing".into()), budget(2, 2, 4)),
        Err(DisplayError::Topology(Error::DisplayNotFound { .. }))
    ));
    assert_eq!(calls.get(), 0);
}

#[test]
fn all_sources_are_budgeted_before_any_capture_even_with_tiny_output() {
    let (mut b, calls) = backend(
        vec![vec![
            d("a", true, 0, 0, 2, 2, 1.0),
            d("b", false, 2, 0, 100, 100, 1.0),
        ]],
        vec![],
        MemoryBudget::new(100, 1_000_000).unwrap(),
    );
    assert!(matches!(
        b.observe(Selection::Desktop, budget(102, 100, 10200)),
        Err(DisplayError::BudgetExceeded {
            kind: "source_bytes",
            ..
        })
    ));
    assert_eq!(calls.get(), 0);
}

#[test]
fn total_peak_accounts_for_native_dib_and_rgba_while_canvas_is_alive() {
    // Canvas=16, DIB=16, source RGBA=16 => 48; PNG phase also counts canvas+PNG.
    let (mut b, calls) = backend(
        vec![vec![d("a", true, 0, 0, 2, 2, 1.0)]],
        vec![],
        MemoryBudget::new(16, 47).unwrap(),
    );
    assert!(matches!(
        b.observe(Selection::Primary, budget(2, 2, 4)),
        Err(DisplayError::BudgetExceeded {
            kind: "total_bytes",
            ..
        })
    ));
    assert_eq!(calls.get(), 0);
}

#[test]
fn source_dimensions_mismatch_discards_everything() {
    let (mut b, _) = backend(
        vec![vec![d("a", true, 0, 0, 2, 2, 1.0)]],
        vec![Ok(solid(1, 2, [1, 2, 3, 255]))],
        memory(),
    );
    assert!(matches!(
        b.observe(Selection::Primary, budget(2, 2, 4)),
        Err(DisplayError::Topology(Error::SourceSizeMismatch { .. }))
    ));
}

#[test]
fn provider_failure_cannot_return_partial_desktop() {
    let (mut b, calls) = backend(
        vec![vec![
            d("a", true, 0, 0, 2, 2, 1.0),
            d("b", false, 2, 0, 2, 2, 1.0),
        ]],
        vec![
            Ok(solid(2, 2, [1, 2, 3, 255])),
            Err(DisplayError::Native {
                operation: "capture",
                code: 5,
            }),
        ],
        memory(),
    );
    assert_eq!(
        b.observe(Selection::Desktop, budget(4, 2, 8)).unwrap_err(),
        DisplayError::Native {
            operation: "capture",
            code: 5
        }
    );
    assert_eq!(calls.get(), 2);
}

#[test]
fn topology_changes_after_capture_discard_pixels_and_png() {
    let a = d("a", true, 0, 0, 2, 2, 1.0);
    for after in [
        {
            let mut d = a.clone();
            d.scale = 1.25;
            d
        },
        {
            let mut d = a.clone();
            d.rotation = Rotation::Deg180;
            d
        },
        {
            let mut d = a.clone();
            d.native_bounds.x = -2;
            d
        },
        {
            let mut d = a.clone();
            d.id = "replacement".into();
            d
        },
    ] {
        let (mut b, _) = backend(
            vec![vec![a.clone()], vec![after]],
            vec![Ok(solid(2, 2, [1, 2, 3, 255]))],
            memory(),
        );
        assert!(matches!(
            b.observe(Selection::Primary, budget(2, 2, 4)),
            Err(DisplayError::Topology(Error::StaleGeneration { .. }))
        ));
    }
}

#[test]
fn unplug_or_no_primary_after_capture_is_not_a_success() {
    let main = d("a", true, 0, 0, 2, 2, 1.0);
    let second = d("b", false, 2, 0, 2, 2, 1.0);
    let (mut b, _) = backend(
        vec![vec![main.clone(), second], vec![main]],
        vec![Ok(solid(2, 2, [1, 2, 3, 255]))],
        memory(),
    );
    assert!(matches!(
        b.observe(Selection::Display("b".into()), budget(2, 2, 4)),
        Err(DisplayError::Topology(Error::StaleGeneration { .. }))
    ));
    let a = d("a", true, 0, 0, 2, 2, 1.0);
    let (mut b, _) = backend(
        vec![vec![a], vec![]],
        vec![Ok(solid(2, 2, [1, 2, 3, 255]))],
        memory(),
    );
    assert!(matches!(
        b.observe(Selection::Primary, budget(2, 2, 4)),
        Err(DisplayError::Topology(Error::EmptyTopology))
    ));
}

#[test]
fn nearest_sampling_is_per_tile_not_whole_canvas_filtering() {
    let (mut b, _) = backend(
        vec![vec![
            d("a", true, 0, 0, 4, 2, 1.0),
            d("b", false, 4, 0, 4, 2, 1.0),
        ]],
        vec![
            Ok(frame(
                4,
                2,
                vec![10, 0, 0, 255, 20, 0, 0, 255, 30, 0, 0, 255, 40, 0, 0, 255].repeat(2),
            )),
            Ok(frame(
                4,
                2,
                vec![0, 10, 0, 255, 0, 20, 0, 255, 0, 30, 0, 255, 0, 40, 0, 255].repeat(2),
            )),
        ],
        memory(),
    );
    let out = b.observe(Selection::Desktop, budget(4, 1, 4)).unwrap();
    assert_eq!(
        out.rgba.pixels(),
        &[10, 0, 0, 255, 30, 0, 0, 255, 0, 10, 0, 255, 0, 30, 0, 255]
    );
}

#[test]
fn png_encoder_handles_multiple_stored_blocks_and_exact_rgba_pixels() {
    let a = d("a", true, 0, 0, 257, 67, 1.0);
    let pixels: Vec<u8> = (0..257 * 67 * 4).map(|i| (i % 251) as u8).collect();
    let (mut b, _) = backend(
        vec![vec![a]],
        vec![Ok(frame(257, 67, pixels.clone()))],
        memory(),
    );
    let out = b
        .observe(Selection::Primary, budget(257, 67, 17219))
        .unwrap();
    let decoded = image::load_from_memory_with_format(&out.png, image::ImageFormat::Png)
        .unwrap()
        .into_rgba8();
    assert_eq!(decoded.dimensions(), (257, 67));
    assert_eq!(decoded.as_raw(), &pixels);
}

#[test]
fn wire_ids_are_ascii_and_bounded() {
    for id in ["écran".to_string(), "a".repeat(129)] {
        let (mut b, calls) = backend(vec![vec![d(&id, true, 0, 0, 1, 1, 1.0)]], vec![], memory());
        assert!(matches!(
            b.observe(Selection::Primary, budget(1, 1, 1)),
            Err(DisplayError::InvalidDisplayId { .. })
        ));
        assert_eq!(calls.get(), 0);
    }
}

#[test]
fn png_peak_is_preflighted_before_capture() {
    // 2x2 RGBA=16; two filtered rows=18; zlib=29; PNG=86 => total=102.
    let (mut b, calls) = backend(
        vec![vec![d("a", true, 0, 0, 2, 2, 1.0)]],
        vec![],
        MemoryBudget::new(16, 101).unwrap(),
    );
    assert_eq!(
        b.observe(Selection::Primary, budget(2, 2, 4)).unwrap_err(),
        DisplayError::BudgetExceeded {
            kind: "total_bytes",
            required: 102,
            limit: 101
        }
    );
    assert_eq!(calls.get(), 0);
}

#[test]
fn reordered_complete_enumeration_keeps_the_observation_generation() {
    let a = d("a", true, 0, 0, 2, 2, 1.0);
    let b = d("b", false, 2, 0, 2, 2, 1.5);
    let (mut backend, _) = backend(
        vec![vec![b.clone(), a.clone()], vec![a, b]],
        vec![Ok(solid(2, 2, [1, 2, 3, 255]))],
        memory(),
    );
    let out = backend
        .observe(Selection::Primary, budget(2, 2, 4))
        .unwrap();
    assert_eq!(out.plan.generation(), out.topology.generation());
    assert_eq!(out.topology.generation().revision(), 1);
}

#[test]
fn malformed_topology_is_rejected_before_capture() {
    let a = d("a", true, 0, 0, 2, 2, 1.0);
    for state in [
        vec![],
        vec![a.clone(), a.clone()],
        vec![d("a", false, 0, 0, 2, 2, 1.0)],
        vec![a, d("b", true, 2, 0, 2, 2, 1.0)],
    ] {
        let (mut b, calls) = backend(vec![state], vec![], memory());
        assert!(matches!(
            b.observe(Selection::Desktop, budget(4, 2, 8)),
            Err(DisplayError::Topology(_))
        ));
        assert_eq!(calls.get(), 0);
    }
}

#[test]
fn native_enumeration_errors_before_or_after_capture_are_preserved() {
    for fail_after_capture in [false, true] {
        let error = DisplayError::Native {
            operation: "EnumDisplayMonitors",
            code: 87,
        };
        let mut states = VecDeque::new();
        let mut frames = VecDeque::new();
        if fail_after_capture {
            states.push_back(Ok(vec![d("a", true, 0, 0, 2, 2, 1.0)]));
            frames.push_back(Ok(solid(2, 2, [1, 2, 3, 255])));
        }
        states.push_back(Err(error.clone()));
        let captures = Rc::new(Cell::new(0));
        let provider = Provider {
            states,
            frames,
            captures: captures.clone(),
        };
        let mut b = DisplayBackend::new(provider, memory()).unwrap();
        assert_eq!(
            b.observe(Selection::Primary, budget(2, 2, 4)).unwrap_err(),
            error
        );
        assert_eq!(captures.get(), usize::from(fail_after_capture));
    }
}
