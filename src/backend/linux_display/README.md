# Linux display adapter boundaries

- Requires an X11 session and RandR **1.5** (logical monitor/root-framebuffer
  geometry, including RandR transforms and panning). Wayland/XWayland fails
  closed. Missing extensions, invalid root geometry, unsupported pixel formats
  and enumeration failures never fall back to guessed single-screen facts.
- `primary`, exact `id`, and `desktop` share the existing pure topology engine.
  **Every selection requires exactly one primary monitor in the complete
  topology.** A sole active monitor with no primary is normalized to primary.
  Multiple monitors with no primary, or conflicting primary claims, are errors
  even for explicit `id`/`desktop`. An explicit-selection escape requires a
  separate shared-library contract change; it is not implemented here.
- Coordinates, source sizes, and native input use root framebuffer pixels.
  Xft font DPI is not consulted. Root images are already oriented; rotation
  metadata must not rotate the pixels a second time.
- The X11 provider implements the portable `rpa-windows-display::FrameProvider`;
  no Windows OS dependencies are compiled on Linux. The engine owns bounded
  composition, source permits, PNG encoding and full topology generation.
  Attachment identities survive reorder and ordinary geometry changes, but are
  retired after observed detach or failed queries. Native state changes not
  expressible in shared geometry also retire identities, so transforms/mode
  changes cannot accidentally retain an old observation generation.
- The provider validates format, dimensions and the engine-issued source permit
  before `GetImage`, then revalidates native state before and after capture.
  Unsupported layouts cannot allocate an unbudgeted native image. Independent
  real-provider read/allocation evidence remains required; pure fixture counters
  alone do **not** establish this property for XCB/the X server.
- Root visuals must be TrueColor (16/24/32-bit storage, checked RGB masks and
  scanline padding). Indexed/DirectColor visuals are rejected, not misdecoded.
- Root-bound rectangles must fit X11 GetImage field ranges. Overlapping logical
  monitors are rejected by the shared desktop composition engine; exact single
  selection still uses the corresponding root rectangle. A logical monitor
  containing differently rotated CRTCs is explicitly rejected.
- No desktop writer lease is acquired here. Host construction passes `X11Lease`
  into native input; `DesktopBackend::new()` refuses unleased Linux creation.
  No server grab, input, global hotkey, or GUI is used by this provider.

## Independent RED and fixture correction

The preserved `cc-root-red/probeC-root-red-run2.log` records the actual independent
assertion RED. Its frozen source in `root-red-dc` already used a 64×64 output
budget. The native source-budget fixture placed the second monitor at x=1100
following a 1024px first monitor: the 76px gap quantized to zero and
`CapturePlan` returned `CollapsedGap` before source-memory preflight. This is
**test-fixture correction, not a product budget bug fixed**. The updated source
budget case uses x=1536 (a representable 512px gap), preserves `resource_limit`
and zero-read/zero-allocation expectations, and separately pins the original
collapsed-gap planning failure. Original RED logs/archives are unchanged.
