# Nested optical chain

A second offscreen capture chain exists in `src/window/backdrop_capture.rs`
and `src/glass/optical/chain.rs`. It is a REAL second-pass compositor, not a
painted proxy: the chain camera republishes the first optical layer's own
output image and re-renders the first layer's attached optical material at
its measured rectangle, so the chain target holds the first layer's actual
refracted, blurred and dispersed pixels. A surface whose nesting depth the
shared theme admits samples that target instead of the painted lower
composite.

## Ownership

- `nesting.rs` derives each live optical surface's zero-based nesting depth
  from Bevy's `ComputedStackIndex` and the measured rectangles the surfaces
  share. It is a geometric fact of the composed UI, not an authored flag.
- `chain.rs` republishes only TOP-LEVEL attached optical surfaces, in their
  real stack order and at their measured physical rectangles. A surface that
  fell back to Painted, or a nested surface itself, is excluded: neither has
  first-layer optical output that belongs under the overlay.
- `backdrop_capture.rs` owns the two offscreen cameras, their two images,
  their resize contract, and the one-frame invalidation controller. The
  chain camera renders after the capture it samples and before the window.
- `capability.rs` remains the one authority for the renderer ceiling. The
  theme consumes the measured depth through `OpticalGlassContext` and fails
  closed at `OPTICAL_GLASS_MAX_NESTED_LAYERS`.

## Honest current state

The chain supplies PIXELS and the GPU proof now measures them. The production
GPU test renders a first optical layer and the chain camera that republishes
it, then asserts the chain target differs from the flat painted backdrop and
captures a second nested-only output target: the second layer really
re-refracts the first layer's own refracted, blurred and dispersed output. On
the development adapter (Intel Arc B390, Vulkan) the standard gate records
`max_nested_layers=2`, `nesting_layers_proven=true`, verdict `PROVEN`.

`OPTICAL_GLASS_MAX_NESTED_LAYERS` is therefore `2`: depth `0` samples the
painted lower composite, depth `1` samples the chain target, and depth `2`
or deeper still fails closed with `NestedLayerLimitExceeded` and paints its
typed Frosted fallback. That fallback is a real degraded material here: the
backdrop is still sampled and blurred (its blur stays inside the proven
ceiling), while the refraction, dispersion, specular rim and shader grain
are off. A Frosted surface with no backdrop to sample paints the tier
instead of claiming a blur it cannot show.

The production WESL now adds a themed Fresnel rim without raising that
ceiling. `OpticalGlassParams` derives Schlick F0 from the shared theme's
`specular_intensity` and `OpticalGlassQuality` lanes; the shader consumes it
as `glass.refraction.y` beside the admitted IOR. The GPU proof owns a named
Fresnel pixel assertion and the receipt records `fresnel_rim_proven=true`
only after that assertion passes. A software/no-adapter/high-contrast host
still fails closed before this material can be claimed.

The shared `bevy_optical_capability_guard.py` and
`docs/QUALITY_GATES.md` now admit the claim only when the proof carries its
named chain assertions and the receipt's ceiling matches `capability.rs`.
The main line still owns `bevy_optical_gpu_receipt.py`'s self-test fixture
and the gate script's fixture line, which may keep one-layer sample strings
as historical classification cases.

What IS proven today:

- `tests/headless/optical_nesting.rs` proves the depth resolver and the
  published depths: an overlay over a card is nested, a card beside it is
  not, and the published resource follows the measured stack.
- `tests/headless/window.rs` proves exactly one chain camera, its order
  between the capture and the presentation, its image target, and that the
  offscreen roots never steal the interactive shell's pointer target.
- `cargo nextest run --locked -p taskmanager-bevy-ui -j 4 --no-fail-fast`
  proves the chain does not disturb the full Bevy headless test suite.

What is NOT proven today: a third or deeper optical layer, and multi-adapter
parity. A targeted hardware `applications-card-lane-optical-menu` capture
shows the production Actions popover over the Applications glass surfaces.
Its readiness checks the mounted menu's attached production material at
depth `1`, a top-level attached glass surface at depth `0`, complete card-lane
bounds, and the active chain camera. The matching compact capture is layout
and bounds evidence only; when hardware WESL capture is disabled, its Painted
fallback is never reported as optical evidence. The GPU proof still runs in a
headless offscreen harness, while the targeted window capture proves the
shipped composition attaches both real materials. The proof ran on one
adapter: the receipt records the adapter it actually ran on, and a machine
without that adapter records `PAINTED` instead.

## Boundary

The ceiling stands at the measured depth, not at the authored one: the
capability guard reads the declared ceiling out of `capability.rs` and
refuses a receipt whose recorded ceiling differs from it, and it refuses any
ceiling above one layer that is not backed by `nesting_layers_proven=true`
plus the proof's own chain assertions. Raising it further requires the same
shape of evidence again: a named GPU proof, a fresh hardware receipt, the
guard released with it, and targeted light/dark captures of a production
overlay above an optical card reviewed by eye.
