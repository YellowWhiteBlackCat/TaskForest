# taskmanager-escalation

## Role

Per-feature privilege-escalation seam with the unprivileged default policy.

## Boundary

The crate owns capability requests, authorization state, exact installed-helper
readiness and the fixed Linux polkit/pkexec crossings. It does not grant blanket
privilege or execute provider I/O. The Windows UAC transport seam (ADR-035
stage 2) owns the typed transport-fact vocabulary, the pure fact→outcome
mapping, the install-fact readiness probe, and the pure launch layer (helper
command-line builder + one-shot reply-channel naming); the runas call group
lives in the audited `taskmanager-windows-api` boundary and the production
driver in `taskmanager-platform-windows`, both feeding this mapping — the
crossing is compile-verified for `x86_64-pc-windows-msvc` but not yet packaged
or on-box receipted. macOS authorization is typed in the `authorization` module
(transport facts, pure mappings, install probe; `aarch64-apple-darwin`
cross-check passes for this crate); the Security-framework crossing itself
stays unwired (`Unsupported`) until a signed privileged-helper ADR exists.

## Contract and verification

Every feature is direct, requires escalation or unsupported. Prompt refusal,
authorization-service failure, missing installation and helper protocol failure
are distinct outcomes. Readiness requires the exact `pkexec + policy + executable
helper` triple and never launches a prompt; the Windows readiness gate reads
install facts only (missing install is `HelperUnavailable`, never a prompt).
Verify fixture classification plus explicit on-box success/denial receipts.

## Module map

```text
src/authorization.rs       typed authorization vocabulary (starts UnprivilegedGate)
src/polkit/gate.rs         PolkitGate: pkexec + policy + executable verification
│                          (the only Linux authorization authority)
├── polkit/{msr,rapl,smbios,process_control,net_launcher,setup}.rs
│                          per-capability helper clients
└── polkit/bounded_runner.rs  polkit/json_reader.rs   bounded execution and envelopes
src/uac.rs                 Windows UAC runas transport vocabulary (ADR-035)
```
