use rpa_display_topology::PixelSize;
use rpa_windows_display::*;

#[test]
fn top_down_bgra_is_opaque_rgba_without_row_reversal() {
    let rgba = RgbaFrame::from_bgra32(
        PixelSize {
            width: 1,
            height: 2,
        },
        4,
        vec![3, 2, 1, 0, 6, 5, 4, 127],
        RowOrder::TopDown,
    )
    .unwrap();
    assert_eq!(rgba.pixels(), &[1, 2, 3, 255, 4, 5, 6, 255]);
}
#[test]
fn bottom_up_conversion_and_bad_stride_or_length_are_explicit() {
    let rgba = RgbaFrame::from_bgra32(
        PixelSize {
            width: 1,
            height: 2,
        },
        4,
        vec![3, 2, 1, 0, 6, 5, 4, 0],
        RowOrder::BottomUp,
    )
    .unwrap();
    assert_eq!(rgba.pixels(), &[4, 5, 6, 255, 1, 2, 3, 255]);
    assert!(matches!(
        RgbaFrame::from_bgra32(
            PixelSize {
                width: 1,
                height: 1
            },
            8,
            vec![0; 8],
            RowOrder::TopDown
        ),
        Err(DisplayError::InvalidStride { .. })
    ));
    assert!(matches!(
        RgbaFrame::new(
            PixelSize {
                width: 2,
                height: 2
            },
            vec![0; 15]
        ),
        Err(DisplayError::InvalidFrameLength { .. })
    ));
}
#[test]
fn size_arithmetic_overflow_and_zero_budgets_are_rejected() {
    assert!(matches!(
        RgbaFrame::new(
            PixelSize {
                width: u32::MAX,
                height: u32::MAX
            },
            Vec::new()
        ),
        Err(DisplayError::ArithmeticOverflow { .. })
    ));
    assert_eq!(
        MemoryBudget::new(0, 1).unwrap_err(),
        DisplayError::InvalidMemoryBudget
    );
    assert_eq!(
        MemoryBudget::new(1, 0).unwrap_err(),
        DisplayError::InvalidMemoryBudget
    );
    assert!(matches!(
        RgbaFrame::new(
            PixelSize {
                width: 0,
                height: 1
            },
            Vec::new()
        ),
        Err(DisplayError::InvalidFrameSize)
    ));
}

#[test]
fn png_and_base64_estimates_have_exact_small_hand_written_lengths() {
    assert_eq!(
        estimate_encoded_size(PixelSize {
            width: 1,
            height: 1
        })
        .unwrap(),
        EncodedSize {
            png_bytes: 73,
            base64_bytes: 100
        }
    );
    assert_eq!(
        estimate_encoded_size(PixelSize {
            width: 2,
            height: 2
        })
        .unwrap(),
        EncodedSize {
            png_bytes: 86,
            base64_bytes: 116
        }
    );
}

#[test]
fn large_transport_estimates_do_not_allocate_or_hide_base64_growth() {
    assert_eq!(
        estimate_encoded_size(PixelSize {
            width: 1920,
            height: 1080
        })
        .unwrap(),
        EncodedSize {
            png_bytes: 8_296_178,
            base64_bytes: 11_061_572
        }
    );
    assert_eq!(
        estimate_encoded_size(PixelSize {
            width: 1920,
            height: 1440
        })
        .unwrap(),
        EncodedSize {
            png_bytes: 11_061_548,
            base64_bytes: 14_748_732
        }
    );
    assert!(matches!(
        estimate_encoded_size(PixelSize {
            width: u32::MAX,
            height: u32::MAX
        }),
        Err(DisplayError::ArithmeticOverflow { .. })
    ));
}
