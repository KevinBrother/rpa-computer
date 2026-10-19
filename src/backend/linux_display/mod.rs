//! X11 root-pixel display provider. Platform-neutral topology and image-format
//! validation stay separate from the RandR/GetImage transport. No input, GUI,
//! selection ownership, or Windows OS API dependencies live here.

mod identity;
#[cfg(target_os = "linux")]
mod native;
mod pixels;
mod topology;
#[cfg(target_os = "linux")]
pub(crate) use native::X11Provider;

#[cfg(test)]
mod tests;
