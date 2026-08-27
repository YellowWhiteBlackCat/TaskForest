# ADR-007: Capability facets and runtime provider registries

Status: accepted

## Context

TaskForest supports several operating systems and hardware families. Platform
selection is a composition concern; hardware, driver, privilege and hot-plug
selection are runtime concerns. A single platform-sized trait would hide
partial availability and allow one slow provider to block unrelated facts.

## Decision

Split the system by ownership and change reason:

| Layer | Owns | Must not own |
|---|---|---|
| `taskmanager-core` | domain facts, identity, availability, lifecycle and pure rules | OS I/O, queues, frontend state |
| `taskmanager-telemetry-store` | bounded, gap-aware time-series read models | refresh policy, provider selection, frontend mutation |
| `taskmanager-platform-contract` | capability IDs, typed requests/events, failures and provider-neutral ports | domain DTOs, UI concepts, OS APIs |
| `taskmanager-application` | commands, request correlation, reducers, projections and bounded host jobs | `/proc`, `/sys`, native commands and vendor SDKs |
| `taskmanager-platform-provider` | blocking provider SPI and capability registration | OS discovery, scheduling and UI state |
| `taskmanager-platform-runtime` | bounded lanes, fairness, correlation and delivery | provider implementations and frontend rendering |
| native platform crates | OS sources, adapters, runtime construction and error mapping | shared queue mechanics and product SKU selection |
| app-host/composition | selects the platform adapter and frontend | a second domain model |

Each capability is a named facet with an explicit provider, lane, request and
failure vocabulary. Independent facets may publish independently; a missing or
failed facet never manufactures a zero value or removes unrelated rows.

The registry is closed by product composition but open to runtime capability
discovery. One standard binary can discover Intel, AMD, NVIDIA, NVMe, ATA,
Wi-Fi and other supported capabilities without vendor-specific Cargo features,
package names or release variants.

## Availability and identity

Providers report `Current`, `Stale`, `Partial`, `Unavailable`, `PermissionDenied`,
`Unsupported` or confirmed empty/zero through typed domain values. A successful
empty result is never inferred from a provider error. Device identity and
generation accompany lifecycle-sensitive observations, so hot-plug, reordering,
counter rollback and PID reuse cannot inherit a prior baseline or control result.

The application owns request correlation and batch projection. Runtime fairness,
queue order and struct layout do not become implicit business ordering. Frontends
consume the same cached projection and do not read platform sources directly.

## Consequences

- Slow or privileged providers remain isolated from ordinary telemetry.
- Platform differences stay visible and honest instead of becoming Linux-shaped
  success values on other systems.
- Adding a provider changes its native registry and contract tests, not the
  shared domain model or every frontend.
- Hardware support is a runtime capability of the standard artifact, not a set
  of vendor-specific packages.

## Verification

Architecture tests enforce the dependency direction, provider registration and
frontend I/O firewall. Behavior tests cover partial discovery, provider failure,
identity/generation replacement, bounded delivery and stale completion rejection.
Target-platform claims still require the corresponding native workflow; a
cross-target compile or fixture does not claim real hardware support.
