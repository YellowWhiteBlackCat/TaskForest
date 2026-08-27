# ADR 006: Separate platform builds from hardware capability

## Status

Accepted.

## Context

TaskManager produces native application versions for different operating
systems. That platform boundary does not imply separate Intel, AMD, NVIDIA, ATA,
NVMe, or other hardware editions. Requiring users to select a GPU-specific Cargo
feature would make hardware support a packaging choice, prevent one binary from
surviving device changes, and make mixed-vendor systems impossible to model
honestly.

## Decision

1. OS adapters are selected at the executable composition edge and implement
   platform-neutral application ports. Shared UI state stores only those ports.
2. Every standard artifact enables `hardware-all` and contains every hardware
   backend supported by that OS version.
3. Hardware backends use runtime discovery. Optional vendor libraries are
   dynamically loaded; no device, missing library, permission denial, stale
   data, and hot removal remain typed runtime outcomes.
4. Vendor-named Cargo features may exist only for isolated development,
   compatibility, and fallback tests. They are not supported distribution SKUs
   or user installation choices.
5. Release builds without `hardware-all` fail at compile time. Packaging and
   documentation tests prevent vendor-specific build commands from returning.
6. A compiled backend proves only that probing is possible. Hardware claims
   still require target-host live receipts with stable identity, partial
   failure, recovery, and hotplug evidence.

The feature matrix is also an executable architecture contract. For Linux,
macOS, and Windows, each adapter's default feature must select
`hardware-all`; every production hardware feature declared by that adapter
must be a member of `hardware-all`; the native target selector must route all
three adapter registries; and the product root must route to the native
registry. Adding an optional hardware dependency without extending this chain
therefore fails the architecture test instead of silently creating an
incomplete standard artifact.

## Consequences

The standard binary is slightly larger, but users install one platform package
that handles mixed and changing hardware. Provider code must merge overlapping
sources by stable device identity rather than vendor/model strings. Platform
ports remain reusable by GPUI and TUI, while hardware support can grow without
creating new product variants.
