//! Direct tests of the pure production RandR adapter and ZPixmap decoder.
use crate::backend::linux_display::{
    pixels::Format,
    topology::{descriptors, Monitor},
};
use rpa_display_topology::{NativeRect, PixelSize, Rotation};

fn monitor(name: u32, primary: bool, x: i32) -> Monitor {
    Monitor {
        name,
        primary,
        bounds: NativeRect {
            x,
            y: 0,
            width: 8,
            height: 6,
        },
        outputs: vec![name + 100],
        rotation: Rotation::Deg0,
    }
}
fn root() -> PixelSize {
    PixelSize {
        width: 40,
        height: 20,
    }
}
fn rgb32() -> Format {
    Format {
        bits_per_pixel: 32,
        scanline_pad: 32,
        little_endian: true,
        red: 0xff0000,
        green: 0xff00,
        blue: 0xff,
    }
}
#[test]
fn native_topology_single_primary_fallback_and_ambiguity() {
    let single = descriptors(1, root(), &[monitor(10, false, 0)]).unwrap();
    assert!(single[0].is_primary);
    assert_eq!(single[0].scale, 1.0);
    assert_eq!(
        single[0].capture_size,
        PixelSize {
            width: 8,
            height: 6
        }
    );
    assert!(descriptors(1, root(), &[monitor(10, false, 0), monitor(20, false, 10)]).is_err());
    assert!(descriptors(1, root(), &[monitor(10, true, 0), monitor(20, true, 10)]).is_err());
}
#[test]
fn native_identity_is_order_independent_and_tracks_output_membership() {
    let a = monitor(10, true, 0);
    let mut b = monitor(20, false, 10);
    let before = descriptors(1, root(), &[a.clone(), b.clone()]).unwrap();
    assert_eq!(
        before,
        descriptors(1, root(), &[b.clone(), a.clone()]).unwrap()
    );
    b.outputs = vec![999];
    let after = descriptors(1, root(), &[a, b]).unwrap();
    assert_ne!(before[1].id, after[1].id);
    assert!(before
        .iter()
        .all(|descriptor| descriptor.id.is_ascii() && descriptor.id.len() <= 128));
}
#[test]
fn native_bounds_fail_closed_instead_of_wrapping_getimage_fields() {
    for bounds in [
        NativeRect {
            x: -1,
            y: 0,
            width: 8,
            height: 6,
        },
        NativeRect {
            x: 36,
            y: 0,
            width: 8,
            height: 6,
        },
        NativeRect {
            x: 0,
            y: 19,
            width: 8,
            height: 6,
        },
        NativeRect {
            x: 0,
            y: 0,
            width: 0,
            height: 6,
        },
        NativeRect {
            x: 32768,
            y: 0,
            width: 1,
            height: 1,
        },
    ] {
        let mut m = monitor(1, true, 0);
        m.bounds = bounds;
        assert!(descriptors(1, root(), &[m]).is_err());
    }
}
#[test]
fn decode_truecolor_obeys_byte_order_and_forces_opaque_alpha() {
    let size = PixelSize {
        width: 2,
        height: 1,
    };
    let little = rgb32()
        .layout(size)
        .unwrap()
        .decode(&[3, 2, 1, 0, 30, 20, 10, 99])
        .unwrap();
    assert_eq!(little.pixels(), &[1, 2, 3, 255, 10, 20, 30, 255]);
    let big = Format {
        little_endian: false,
        ..rgb32()
    }
    .layout(size)
    .unwrap()
    .decode(&[0, 1, 2, 3, 99, 10, 20, 30])
    .unwrap();
    assert_eq!(big.pixels(), little.pixels());
}
#[test]
fn decode_rgb565_and_padded_24bit_rows() {
    let format = Format {
        bits_per_pixel: 16,
        red: 0xf800,
        green: 0x07e0,
        blue: 0x001f,
        ..rgb32()
    };
    let decoded = format
        .layout(PixelSize {
            width: 2,
            height: 1,
        })
        .unwrap()
        .decode(&[0, 0xf8, 0xe0, 7])
        .unwrap();
    assert_eq!(decoded.pixels(), &[255, 0, 0, 255, 0, 255, 0, 255]);
    let layout = Format {
        bits_per_pixel: 24,
        ..rgb32()
    }
    .layout(PixelSize {
        width: 1,
        height: 2,
    })
    .unwrap();
    assert_eq!(layout.stride, 4);
    assert_eq!(layout.wire_bytes, 8);
    assert_eq!(
        layout.decode(&[3, 2, 1, 99, 6, 5, 4, 99]).unwrap().pixels(),
        &[1, 2, 3, 255, 4, 5, 6, 255]
    );
}
#[test]
fn malformed_image_format_or_payload_is_not_relabeled_as_rgba() {
    let size = PixelSize {
        width: 2,
        height: 1,
    };
    for format in [
        Format { red: 0, ..rgb32() },
        Format {
            green: 0xff0000,
            ..rgb32()
        },
        Format {
            red: 0x00550000,
            ..rgb32()
        },
        Format {
            bits_per_pixel: 8,
            ..rgb32()
        },
        Format {
            scanline_pad: 7,
            ..rgb32()
        },
    ] {
        assert!(format.layout(size).is_err());
    }
    let layout = rgb32().layout(size).unwrap();
    assert!(layout.decode(&[0; 7]).is_err());
    assert!(layout.decode(&[0; 9]).is_err());
}

#[test]
fn attachment_ids_survive_reorder_not_observed_detach_or_query_failure() {
    use crate::backend::linux_display::identity::Identities;
    let mut ids = Identities::default();
    let raw = descriptors(1, root(), &[monitor(10, true, 0), monitor(20, false, 10)]).unwrap();
    let first = ids.reconcile(&raw).unwrap();
    assert!(ids.matches(&raw[0], &first[0]));
    let mut reordered = raw.clone();
    reordered.reverse();
    assert_eq!(first, ids.reconcile(&reordered).unwrap());
    ids.reconcile(&raw[..1]).unwrap();
    let replug = ids.reconcile(&raw).unwrap();
    assert_eq!(first[0].id, replug[0].id);
    assert_ne!(first[1].id, replug[1].id);
    ids.invalidate();
    assert_ne!(replug[0].id, ids.reconcile(&raw).unwrap()[0].id);
}

#[test]
fn decoder_admission_rejects_wrong_shape_even_when_byte_counts_match() {
    use rpa_display_topology::{CaptureBudget, DisplayDescriptor, Selection};
    use rpa_windows_display::{
        DisplayBackend, DisplayError, FrameProvider, MemoryBudget, RgbaFrame, SourcePermit,
    };
    struct Probe;
    impl FrameProvider for Probe {
        fn enumerate(&mut self) -> Result<Vec<DisplayDescriptor>, DisplayError> {
            Ok(vec![DisplayDescriptor {
                id: "permit".into(),
                is_primary: true,
                native_bounds: NativeRect {
                    x: 0,
                    y: 0,
                    width: 1,
                    height: 2,
                },
                capture_size: PixelSize {
                    width: 1,
                    height: 2,
                },
                scale: 1.0,
                rotation: Rotation::Deg0,
            }])
        }
        fn capture(
            &mut self,
            _: &DisplayDescriptor,
            permit: SourcePermit,
        ) -> Result<RgbaFrame, DisplayError> {
            let same_bytes_wrong_shape = rgb32()
                .layout(PixelSize {
                    width: 2,
                    height: 1,
                })
                .unwrap();
            assert!(same_bytes_wrong_shape.require(permit).is_err());
            let admitted = rgb32().layout(permit.size()).unwrap().require(permit)?;
            admitted.decode(&[0; 8])
        }
    }
    let mut engine = DisplayBackend::new(Probe, MemoryBudget::new(8, 256).unwrap()).unwrap();
    let observation = engine
        .observe(Selection::Primary, CaptureBudget::new(1, 2, 2).unwrap())
        .unwrap();
    assert_eq!(observation.rgba.pixels(), &[0, 0, 0, 255, 0, 0, 0, 255]);
}
