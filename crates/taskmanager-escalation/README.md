# taskmanager-escalation

## Role

Per-feature privilege-escalation seam with the unprivileged default policy.

## Boundary

The crate owns capability requests, authorization state, exact installed-helper
readiness and the fixed Linux polkit/pkexec crossings. It does not grant blanket
privilege or execute provider I/O. The Windows UAC transport vocabulary
(ADR-035) and the typed macOS authorization module live here; the per-platform
transport facts (helper shipping, compile verification, receipt-before-claim)
are specified in [`docs/PLATFORM_TRANSPORTS.md`](docs/PLATFORM_TRANSPORTS.md).

## Contract and verification

Every feature is direct, requires escalation or unsupported. Prompt refusal,
authorization-service failure, missing installation and helper protocol failure
are distinct outcomes; readiness gates, the `HelperUnavailable` rule and the
per-platform transport facts are specified in
[`docs/PLATFORM_TRANSPORTS.md`](docs/PLATFORM_TRANSPORTS.md). Verify fixture
classification plus explicit on-box success/denial receipts.

## Module map

```text
src/authorization.rs       typed authorization vocabulary (starts UnprivilegedGate)
src/polkit/gate.rs         PolkitGate: pkexec + policy + executable verification
│                          (the only Linux authorization authority)
├── polkit/{msr,rapl,smbios,process_control,net_launcher,smart_self_test,setup}.rs
│                          per-capability helper clients
└── polkit/bounded_runner.rs  polkit/json_reader.rs   bounded execution and envelopes
src/uac.rs                 Windows UAC runas transport vocabulary (ADR-035)
```

## Docs

- [`docs/PLATFORM_TRANSPORTS.md`](docs/PLATFORM_TRANSPORTS.md) — Windows UAC, macOS
  authorization and Linux polkit transport detail.
- [`PERMISSION_MODEL`](../../docs/PERMISSION_MODEL.md) and
  [ADR-023](../../adr/023-per-feature-privilege-escalation-framework.md) — permission model
  and escalation framework.
