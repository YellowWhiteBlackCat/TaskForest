# ADR-062: Backdrop-sampled optical glass is capability-gated

Status: accepted (contract and backend disposition); implementation remains
incomplete on every frontend until the task ledger records production evidence

## Context

ADR-061 defines a painted frosted material: theme-owned translucent fill,
gradient sheen, outline, shadow and optional grain. That material does not
sample the already-composited pixels behind an element, so it cannot provide
backdrop blur, rounded-SDF refraction, chromatic dispersion or a
normal-dependent specular rim.

The product uses the term optical glass only when all of those operations are
present. A color gradient, translucent fill, rounded outline, shadow or an
unused blur-radius field is a fallback, not optical glass.

The locked backend APIs were audited under the no-vendor rule:

- GPUI (`gpui-pre 0.3.10`) exposes no sampleable lower composite or generic
  backdrop primitive.
- Iced (`iced_wgpu 0.14.0`) cannot provide a custom primitive with the
  already-rendered backdrop while preserving the required painter order.
- Bevy (`bevy 0.20.1`) can use an offscreen camera and `RenderTarget::Image`
  with `UiMaterial`; an isolated P0 exercised the sampling mechanism with
  public APIs. Its captures do not establish the product's same-frame nested
  composite or grain behavior.

## Decision

1. **The theme owns optical design intent, not renderer claims.**
   `taskmanager-theme::OpticalGlassSpec` is toolkit-neutral and dependency
   free. Its geometry is authored in REM; `OpticalGlassSpec::resolve` converts
   the live root size and device scale factor into physical pixels for the
   renderer binding. Pages do not own blur, refraction or dispersion
   constants.

2. **Capability is explicit and fail-closed.** `OpticalGlassCapability` has no
   default-on value. A production renderer must report backdrop sampling,
   blur, refraction, dispersion, specular, grain, maximum simultaneous
   nesting depth and maximum physical blur radius.
   `Theme::optical_glass_decision` is the single fallback authority.

3. **Fallback order is semantic.** High contrast returns `Opaque`. A headless
   render or missing sampleable backdrop returns `Painted`. Backdrop blur
   without the complete optical chain, or a nesting/blur limit overflow,
   returns `Frosted` when sampling and blur exist and `Painted` otherwise.
   Every fallback carries a typed reason.

4. **A mechanism spike is not a production capability.** Bevy's P0 verdict is
   `GO` for the mechanism only. The real Bevy window reports a capability only
   when `RenderPlugin` publishes a live `RenderAdapterInfo`; without one it
   reports `OpticalGlassCapability::UNSUPPORTED` and the theme returns Painted.
   The production WESL, its invalidation rules and single-layer backdrop
   composite are proved by the standard `bevy-optical-gpu` gate against the
   current source digest. GPUI and Iced report `UNSUPPORTED` against their
   locked versions.

5. **No supply-chain workaround is allowed.** Optical glass may not add a
   vendor tree, fork, `[patch]`, private branch or local renderer patch. If the
   released public API cannot provide the required composite, the backend
   verdict is `NO_GO` and remains on the typed fallback until upstream
   publishes a suitable API and the P0 gate is rerun.

6. **No relabelling.** A renderer may use the optical-glass label only for an
   `OpticalGlassDecision::Optical` produced by a complete capability report.
   Painted and frosted frames remain labelled as fallbacks in code, captures
   and documentation.

## Consequences

- ADR-061 remains the current painted frosted material contract.
- Bevy can proceed with production integration behind the capability gate
  without claiming completion early.
- Bevy owns an offscreen `BackdropCaptureCamera` scaffold and a primary-window
  `PresentationCamera` for the interactive shell. Performance marks a
  candidate card; the GPU gate proves the production WESL's blur, refraction,
  dispersion, specular, grain, opacity, rounded clipping and lower-scene
  tracking on a live adapter. A software adapter and a missing adapter both
  remain `PAINTED` in the private receipt. Sustained-window performance and
  real Wayland/capture evidence for the optical path remain separate gates.
- GPUI and Iced keep honest fallbacks and become eligible only after an
  upstream release restores the required public API.
- Theme tests cover every skin × Light/Dark/EyeForest × high-contrast fallback
  decision, and renderer bindings pin their current capability reports.
- Optical evidence must include real backdrop sampling, measurable blur and
  edge displacement, channel dispersion, correct rounded clipping and real
  captures. A pure-color screenshot cannot prove optical glass. The current
  GPU receipt proves one top-level glass (`max_nested_layers=1`); nested
  optical layers remain unproven and fail closed through
  `OpticalGlassUnavailableReason::NestedLayerLimitExceeded`.
