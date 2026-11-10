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
it, then asserts the chain target differs from the flat painted backdrop by
more than a fixed threshold: the second layer really samples the first
layer's own refracted, blurred and dispersed output. On the development
adapter (Intel Arc B390, Vulkan) the standard gate records
`max_nested_layers=2`, `nesting_layers_proven=true`, verdict `PROVEN`.

`OPTICAL_GLASS_MAX_NESTED_LAYERS` is therefore `2`: depth `0` samples the
painted lower composite, depth `1` samples the chain target, and depth `2`
or deeper still fails closed with `NestedLayerLimitExceeded` and paints its
typed Frosted fallback. The shared `bevy_optical_capability_guard.py` and
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
  proves the chain does not disturb the 682-test headless composition.

What is NOT proven today: a third or deeper optical layer, and a reviewed
real-window capture of two simultaneously optical layers. The GPU proof is
a headless, offscreen harness that measures pixels; it is not the shipped
window. The `tooltip-overlay` and `applications-card-lane` targeted
captures passed their matrix validation and were reviewed by eye, but the
visible difference is a small "Choose columns" tooltip — they do not
establish that two optical layers were in frame. The hardware run above is
also a single adapter: the receipt records the adapter it actually ran on,
and a machine without that adapter records `PAINTED` instead.

## Boundary

The ceiling stands at the measured depth, not at the authored one: the
capability guard reads the declared ceiling out of `capability.rs` and
refuses a receipt whose recorded ceiling differs from it, and it refuses any
ceiling above one layer that is not backed by `nesting_layers_proven=true`
plus the proof's own chain assertions. Raising it further requires the same
shape of evidence again: a named GPU proof, a fresh hardware receipt, the
guard released with it, and targeted light/dark captures of an overlay above
an optical card reviewed by eye.
