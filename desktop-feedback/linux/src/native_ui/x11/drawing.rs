//! Pango Unicode shaping + Cairo antialiasing, with bounded CPU image surfaces.
//! These are renderer pixels, not captured desktop pixels. Never sample a screen.
use super::*;
use cairo::{Context, Format as CairoFormat, ImageSurface};
use rpa_desktop_feedback_linux::model::Severity;
use std::f64::consts::PI;
fn color(c: &Context, rgb: [u8; 3]) {
    c.set_source_rgb(
        f64::from(rgb[0]) / 255.,
        f64::from(rgb[1]) / 255.,
        f64::from(rgb[2]) / 255.,
    );
}
fn rounded(c: &Context, w: f64, h: f64, r: f64) {
    c.new_sub_path();
    c.arc(w - r, r, r, -PI / 2., 0.);
    c.arc(w - r, h - r, r, 0., PI / 2.);
    c.arc(r, h - r, r, PI / 2., PI);
    c.arc(r, r, r, PI, PI * 1.5);
    c.close_path();
}
fn text(
    c: &Context,
    value: &str,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    size: f64,
    rgb: [u8; 3],
    bold: bool,
    centered: bool,
) -> Result<(), UiFailure> {
    if !width.is_finite() || width <= 0. || width > 32767. || height <= 0. {
        return Err(UiFailure::Geometry);
    }
    c.save().map_err(|_| UiFailure::Native)?;
    c.rectangle(x, y, width, height);
    c.clip();
    let layout = pangocairo::functions::create_layout(c);
    layout.set_text(value); // never Pango markup
    layout.set_width((width * f64::from(pango::SCALE)).floor() as i32);
    layout.set_ellipsize(pango::EllipsizeMode::End);
    layout.set_single_paragraph_mode(true);
    layout.set_auto_dir(true);
    if centered {
        layout.set_alignment(pango::Alignment::Center);
    }
    let mut font = pango::FontDescription::new();
    font.set_family("Sans");
    font.set_absolute_size(size * f64::from(pango::SCALE));
    font.set_weight(if bold {
        pango::Weight::Semibold
    } else {
        pango::Weight::Normal
    });
    layout.set_font_description(Some(&font));
    if layout.unknown_glyphs_count() != 0 {
        return Err(UiFailure::InvalidStyle); // missing font coverage is not Unicode success
    }
    let (_, ink_height) = layout.pixel_size();
    c.move_to(x, y + ((height - f64::from(ink_height)) / 2.).max(0.));
    color(c, rgb);
    pangocairo::functions::show_layout(c, &layout);
    c.restore().map_err(|_| UiFailure::Native)?;
    Ok(())
}
pub(super) fn draw(
    role: Role,
    scene: &Scene,
    width: u16,
    height: u16,
    scale: f64,
    format: Format,
) -> Result<Raster, UiFailure> {
    let size = usize::from(width)
        .checked_mul(usize::from(height))
        .and_then(|v| v.checked_mul(4))
        .ok_or(UiFailure::Geometry)?;
    if size == 0 || size > 8 * 1024 * 1024 {
        return Err(UiFailure::Geometry);
    }
    let mut image = ImageSurface::create(CairoFormat::ARgb32, width.into(), height.into())
        .map_err(|_| UiFailure::Native)?;
    let c = Context::new(&image).map_err(|_| UiFailure::Native)?;
    c.set_operator(cairo::Operator::Clear);
    c.paint().map_err(|_| UiFailure::Native)?;
    c.set_operator(cairo::Operator::Over);
    let theme = palette(scene.style.accent);
    let (w, h) = (f64::from(width), f64::from(height));
    let accent = match scene.presentation.severity {
        Severity::Normal => scene.style.accent,
        Severity::Warning => [0xfb, 0xbf, 0x24],
        Severity::Error => [0xf8, 0x71, 0x71],
    };
    match role {
        Role::Status => {
            rounded(&c, w, h, 10. * scale);
            color(&c, theme.surface);
            c.fill().map_err(|_| UiFailure::Native)?;
            c.save().map_err(|_| UiFailure::Native)?;
            c.translate(0.5, 0.5);
            rounded(&c, w - 1., h - 1., 10. * scale);
            color(&c, theme.border);
            c.set_line_width(1.);
            c.stroke().map_err(|_| UiFailure::Native)?;
            c.restore().map_err(|_| UiFailure::Native)?;
            c.arc(17. * scale, 24. * scale, 4. * scale, 0., 2. * PI);
            color(&c, accent);
            c.fill().map_err(|_| UiFailure::Native)?;
            let x = 30. * scale;
            let available = w - x - 12. * scale;
            text(
                &c,
                &scene.style.label,
                x,
                6. * scale,
                available,
                18. * scale,
                13. * scale,
                [255; 3],
                true,
                false,
            )?;
            text(
                &c,
                scene.presentation.text,
                x,
                24. * scale,
                available,
                18. * scale,
                11. * scale,
                theme.secondary_text,
                false,
                false,
            )?;
        }
        Role::Stop => {
            let fill = match scene.stop_visual {
                ButtonVisual::Normal => theme.stop_normal,
                ButtonVisual::Hover => theme.stop_hover,
                ButtonVisual::Pressed => theme.stop_pressed,
                ButtonVisual::Disabled => theme.stop_disabled,
            };
            color(&c, fill);
            // On bare X11 retain a full opaque rectangle so the entire 88x48
            // hit region (including corners) remains reachable.
            if format.argb {
                rounded(&c, w, h, 10. * scale);
            } else {
                c.rectangle(0., 0., w, h);
            }
            c.fill().map_err(|_| UiFailure::Native)?;
            text(
                &c,
                theme.stop_label,
                6. * scale,
                0.,
                w - 12. * scale,
                h,
                13. * scale,
                theme.stop_text,
                true,
                true,
            )?;
        }
        Role::Ring => {
            c.arc(w / 2., h / 2., 16. * scale, 0., 2. * PI);
            c.set_line_width(5. * scale);
            color(&c, [255; 3]);
            c.stroke().map_err(|_| UiFailure::Native)?;
            c.arc(w / 2., h / 2., 16. * scale, 0., 2. * PI);
            c.set_line_width(2. * scale);
            color(&c, scene.style.accent);
            c.stroke().map_err(|_| UiFailure::Native)?;
        }
    }
    c.status().map_err(|_| UiFailure::Native)?;
    drop(c);
    image.flush();
    let stride = image.stride() as usize;
    let bytes = image.data().map_err(|_| UiFailure::Native)?;
    let mut pixels = vec![0; size];
    let mut shape = Vec::new();
    for y in 0..usize::from(height) {
        let mut start = None;
        for x in 0..usize::from(width) {
            let at = y * stride + x * 4;
            let argb = u32::from_ne_bytes(
                bytes[at..at + 4]
                    .try_into()
                    .map_err(|_| UiFailure::Native)?,
            );
            let alpha = argb >> 24;
            let visible = alpha >= 128;
            if visible && start.is_none() {
                start = Some(x);
            }
            if !visible {
                if let Some(from) = start.take() {
                    shape.push(x::Rectangle {
                        x: from as i16,
                        y: y as i16,
                        width: (x - from) as u16,
                        height: 1,
                    });
                }
            }
            let native = ((argb >> 16) & 255) << format.shifts[0]
                | ((argb >> 8) & 255) << format.shifts[1]
                | (argb & 255) << format.shifts[2]
                | if format.argb { alpha << 24 } else { 0 };
            let out = y * usize::from(width) * 4 + x * 4;
            pixels[out..out + 4].copy_from_slice(&if format.little {
                native.to_le_bytes()
            } else {
                native.to_be_bytes()
            });
        }
        if let Some(from) = start {
            shape.push(x::Rectangle {
                x: from as i16,
                y: y as i16,
                width: width - from as u16,
                height: 1,
            });
        }
    }
    if shape.len() > 2048 {
        return Err(UiFailure::Geometry);
    }
    Ok(Raster {
        width,
        height,
        pixels,
        shape,
    })
}
