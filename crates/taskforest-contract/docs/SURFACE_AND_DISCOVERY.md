# Capture, discovery, and the platform surface

## Window capture receipt

`window_capture.rs` owns the platform-neutral receipt, backend provenance,
bounded failure vocabulary, in-process renderer hook and native-adapter trait
for one accepted PNG. It contains no Wayland, D-Bus, PipeWire or
provider-specific object, leaving those details to the native adapter. The
backend each target selects today is declared by the
[platform-native selection matrix](../../taskmanager-platform-native/docs/SELECTION_MATRIX.md);
Portal Screenshot, ScreenCaptureKit and continuous ScreenCast/PipeWire remain
explicit future backend values and must not be inferred as supported from
this contract alone.

## Device discovery and failure projection

Physical-device producers use the constrained `DeviceDiscovery` constructor
so IDs, discovery outcome, and item count are derived together; it is the
only public construction route. `RequiresEscalation` survives
provider-to-operation mapping as its own outcome rather than becoming a
generic permission denial. `ProviderFailure::capability_status` is the sole
failure→capability-status projection: the runtime catalog and the conformance
scenarios delegate to it, and a caller holding the core `FailureKind`
vocabulary reaches the same function through the lossless `from_kind`
conversion.

## Static platform surface

`platform_surface.rs` owns the static per-platform registration declaration
the three-axis parity ledger folds against: `Present`, a typed
`Absent(CapabilityStatus)` restricted to the four absence projections, or the
forbidden silent `Undeclared`. It is a declaration, never a runtime status;
runtime availability stays with the catalog snapshot and platform conformance.
`PlatformCapabilitySurface::declaring` is the layer-B constructor adapters use
next to their real route/provider registration: it takes the lanes the
adapter registers with their honest source and pads every other
product-expected identity with the unsupported absence, so the declaration
covers the complete expected face and `Undeclared` never hides a missing
lane.

`platform.rs` owns the parity axis identity: exactly the three shipped
product targets (`linux`/`windows`/`macos`). The Android/OpenHarmony
placeholder seams are outside the axis by design (ADR-043/044/053) and never
resolve from `PlatformAxis::from_id`; admitting a fourth variant is a
conscious hard cutover.
