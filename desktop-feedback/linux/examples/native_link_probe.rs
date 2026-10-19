//! TEST-ONLY LINK TARGET, NOT THE RENDERER. Build; do not run for this checkpoint.
//! References actual locked native bindings, so Linux linking is more meaningful
//! than `cargo check`. No X connection, window, ready or Host integration exists.
#[cfg(all(target_os = "linux", feature = "native-ui"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let image = cairo::ImageSurface::create(cairo::Format::ARgb32, 128, 32)?;
    let context = cairo::Context::new(&image)?;
    let layout = pangocairo::functions::create_layout(&context);
    layout.set_text("控制 · العربية · e\u{301}");
    layout.set_width(128 * pango::SCALE);
    layout.set_ellipsize(pango::EllipsizeMode::End);
    layout.set_single_paragraph_mode(true);
    let mut font = pango::FontDescription::new();
    font.set_family("Sans");
    font.set_absolute_size(13.0 * f64::from(pango::SCALE));
    layout.set_font_description(Some(&font));
    pangocairo::functions::show_layout(&context, &layout);
    std::hint::black_box(layout.pixel_size());
    // Force an XCB link reference, but do not connect to any display.
    std::hint::black_box(
        xcb::Connection::connect
            as fn(Option<&str>) -> Result<(xcb::Connection, i32), xcb::ConnError>,
    );
    Ok(())
}

#[cfg(not(all(target_os = "linux", feature = "native-ui")))]
fn main() {
    eprintln!("native_link_probe requires Linux and --features native-ui; not a renderer");
    std::process::exit(2);
}
