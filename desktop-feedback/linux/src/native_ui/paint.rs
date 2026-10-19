//! Bounded style and drawing requests. Native text shaping lives in x11/drawing.
use super::UiFailure;
use rpa_desktop_feedback_linux::geometry::Rect;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Style {
    pub label: String,
    pub accent: [u8; 3],
}
pub fn parse_style(args: &[&str]) -> Result<Style, UiFailure> {
    let mut style = Style {
        label: "AI Control".into(),
        accent: [0x25, 0x63, 0xeb],
    };
    let (mut label, mut accent) = (false, false);
    let mut pairs = args.chunks_exact(2);
    for pair in &mut pairs {
        match pair[0] {
            "--label" if !label => {
                validate_label(pair[1])?;
                style.label = pair[1].into();
                label = true;
            }
            "--accent" if !accent => {
                let bytes = pair[1].as_bytes();
                if bytes.len() != 7
                    || bytes[0] != b'#'
                    || !bytes[1..].iter().all(u8::is_ascii_hexdigit)
                {
                    return Err(UiFailure::InvalidStyle);
                }
                for (i, channel) in style.accent.iter_mut().enumerate() {
                    *channel = u8::from_str_radix(&pair[1][1 + i * 2..3 + i * 2], 16)
                        .map_err(|_| UiFailure::InvalidStyle)?;
                }
                accent = true;
            }
            _ => return Err(UiFailure::InvalidStyle),
        }
    }
    if !pairs.remainder().is_empty() {
        return Err(UiFailure::InvalidStyle);
    }
    Ok(style)
}
fn validate_label(text: &str) -> Result<(), UiFailure> {
    // Bounded UTF-16, no control or Unicode format characters (Cf), including
    // bidi overrides/isolates, zero-width controls and annotation/tag controls.
    if text.trim().is_empty()
        || text.encode_utf16().count() > 48
        || text.chars().any(|c| {
            c.is_control()
                || matches!(c as u32,
            0x00ad | 0x0600..=0x0605 | 0x061c | 0x06dd | 0x070f | 0x0890..=0x0891 |
            0x08e2 | 0x180e | 0x200b..=0x200f | 0x202a..=0x202e | 0x2060..=0x2064 |
            0x2066..=0x206f | 0xfeff | 0xfff9..=0xfffb | 0x110bd | 0x110cd |
            0x13430..=0x1343f | 0x1bca0..=0x1bca3 | 0x1d173..=0x1d17a |
            0xe0001 | 0xe0020..=0xe007f)
        })
    {
        return Err(UiFailure::InvalidStyle);
    }
    Ok(())
}
#[derive(Debug, Clone, PartialEq)]
pub struct TextRequest {
    pub text: String,
    pub markup: bool,
    pub single_paragraph: bool,
    pub ellipsize_end: bool,
    pub auto_direction: bool,
    pub width_px: u32,
    pub font_px: f64,
}
pub fn text_request(text: &str, clip: Rect, scale: f64) -> Result<TextRequest, UiFailure> {
    if clip.width == 0
        || clip.height == 0
        || clip.width > 32767
        || !scale.is_finite()
        || !(0.5..=4.0).contains(&scale)
    {
        return Err(UiFailure::InvalidStyle);
    }
    Ok(TextRequest {
        text: text.into(),
        markup: false,
        single_paragraph: true,
        ellipsize_end: true,
        auto_direction: true,
        width_px: clip.width,
        font_px: 13.0 * scale,
    })
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    pub surface: [u8; 3],
    pub border: [u8; 3],
    pub secondary_text: [u8; 3],
    pub stop_text: [u8; 3],
    pub stop_normal: [u8; 3],
    pub stop_hover: [u8; 3],
    pub stop_pressed: [u8; 3],
    pub stop_disabled: [u8; 3],
    pub stop_label: &'static str,
}
pub fn palette(_accent: [u8; 3]) -> Palette {
    Palette {
        surface: [0x14, 0x14, 0x16],
        border: [0x5a, 0x5a, 0x60],
        secondary_text: [0xdb, 0xdb, 0xe1],
        stop_text: [0x19, 0x1a, 0x1f],
        stop_normal: [0xf5, 0xf6, 0xfa],
        stop_hover: [0xdf, 0xe6, 0xf5],
        stop_pressed: [0xb9, 0xc9, 0xe8],
        stop_disabled: [0x88, 0x8b, 0x94],
        stop_label: "Stop",
    }
}
