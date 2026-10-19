//! Standalone optional native renderer. Stdout is exclusively the v1 protocol.
#[cfg(target_os = "linux")]
#[path = "../native_ui/mod.rs"]
mod native_ui;
#[cfg(target_os = "linux")]
fn main() {
    // Never echo panic payloads/labels/wire bytes to stderr.
    std::panic::set_hook(Box::new(|_| {}));
    let code = std::panic::catch_unwind(|| {
        let args: Vec<String> = std::env::args().skip(1).collect();
        native_ui::process::run(&args.iter().map(String::as_str).collect::<Vec<_>>())
    })
    .unwrap_or(70);
    std::process::exit(code);
}
#[cfg(not(target_os = "linux"))]
fn main() {
    std::process::exit(65);
}
