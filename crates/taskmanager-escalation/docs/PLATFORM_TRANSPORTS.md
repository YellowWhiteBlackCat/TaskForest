# Platform transports

## Windows UAC (ADR-035)

The Windows UAC transport seam (ADR-035 stage 2) owns the typed
transport-fact vocabulary, the pure fact→outcome mapping, the install-fact
readiness probe, and the pure launch layer (helper command-line builder +
one-shot reply-channel naming). The runas call group lives in the audited
`taskmanager-windows-api` boundary and the production driver in
`taskmanager-platform-windows`, both feeding this mapping — the crossing is
compile-verified for `x86_64-pc-windows-msvc` and the fixed helper is part of
the Windows MSI beside the GPUI executable; native success/denial is not
claimed until a Windows desktop receipt exists. The Windows readiness gate
reads install facts only (missing install is `HelperUnavailable`, never a
prompt).

## macOS authorization

macOS authorization is typed in the `authorization` module (transport facts,
pure mappings, install probe; `aarch64-apple-darwin` cross-check passes for
this crate); the Security-framework crossing itself stays unwired
(`Unsupported`) until a signed privileged-helper ADR exists.

## Linux polkit

Readiness requires the exact `pkexec + policy + executable helper` triple and
never launches a prompt. `src/polkit/gate.rs` (`PolkitGate`) is the only
Linux authorization authority; the per-capability clients under
`src/polkit/` cover the msr, rapl, smbios, process_control, net_launcher,
smart_self_test and setup helpers, with `bounded_runner.rs` and
`json_reader.rs` providing bounded execution and envelopes. The SMART
self-test client passes one fixed target and kind; the helper itself validates
persistent identity and exact capability before issuing the device command.
