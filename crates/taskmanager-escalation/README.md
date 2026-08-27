# taskmanager-escalation

## Role

Per-feature privilege-escalation seam with the unprivileged default policy.

## Boundary

The crate owns capability requests, authorization state, exact installed-helper
readiness and the fixed Linux polkit/pkexec crossings. It does not grant blanket
privilege or execute provider I/O. The Windows UAC transport seam (ADR-035)
owns the typed transport-fact vocabulary, the pure fact→outcome mapping, and
the install-fact readiness probe; the `runas` crossing itself stays unwired
(typed `Unsupported`) until ADR-035 stage 2. macOS authorization remains typed
unsupported/unwired.

## Contract and verification

Every feature is direct, requires escalation or unsupported. Prompt refusal,
authorization-service failure, missing installation and helper protocol failure
are distinct outcomes. Readiness requires the exact `pkexec + policy + executable
helper` triple and never launches a prompt; the Windows readiness gate reads
install facts only (missing install is `HelperUnavailable`, never a prompt).
Verify fixture classification plus explicit on-box success/denial receipts.
