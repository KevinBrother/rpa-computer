//! Actual public SetMap selection and validated payload shared by live/tests.
#[cfg(target_os = "linux")]
use super::super::map_request::NativeMapRequest;
#[cfg(not(target_os = "linux"))]
use crate::linux_x11_map_request::NativeMapRequest;
use crate::Result;
use map_wire::{Payload, PreparedMap};

pub(crate) trait Transport {
    /// Send the validated public SetMap, not XkbChangeMap or a fallback.
    fn set_map(&mut self, prepared: &PreparedMap<'_>) -> Result<()>;
}
pub(crate) fn submit(
    io: &mut impl Transport,
    request: &NativeMapRequest,
    payload: Payload<'_>,
) -> Result<()> {
    let prepared = map_wire::prepare(request, payload)?;
    io.set_map(&prepared)
}

#[path = "map_wire.rs"]
pub(crate) mod map_wire;

#[cfg(test)]
#[path = "map_backend_tests.rs"]
mod map_backend_tests;
