# ADR-009: Honest compile-time selection for incomplete native adapters

Status: accepted

## Context

The executable host already selected Linux through
`taskmanager-platform-native`, but every other target stopped in a broad
`compile_error!`. That proved neither that the application contracts were
portable nor that Windows and macOS could be composed without inheriting Linux
provider identities, paths, commands, or snapshot shapes.

The reusable channel runtime is intentionally designed for independently
optional capabilities. Until that optional-lane construction is the stable
cross-OS API, forcing an incomplete adapter through a full provider-binding
table would be worse than leaving capabilities absent: it would register
providers that do not exist.

## Decision

1. `taskmanager-platform-native` owns only target-specific dependencies and
   re-exports. It selects one physical adapter crate at compile time:
   - Linux selects `LinuxPlatformRuntime`;
   - Windows selects the thin `WindowsPlatformRuntime`;
   - macOS selects the thin `MacOsPlatformRuntime`;
   - an unrecognized target fails at compile time.
2. Windows and macOS own separate
   `taskmanager-platform-windows` and `taskmanager-platform-macos` crates.
   They initially reuse `taskmanager-platform-runtime::capability_absent_handle`,
   which creates:
   - an empty `CapabilitySnapshot`;
   - `PlatformFacets::default()`, with no request ports;
   - an idle event port with no fabricated domain events.
3. A request for any unimplemented facet is rejected by the application client
   as `SubmissionErrorKind::UnsupportedCapability`. The transition runtime
   never creates a provider ID, an `Available` descriptor, or a successful
   zero-valued hardware/telemetry snapshot.
   Constructing this intentionally empty composition is infallible, so the two
   thin adapters return `Result<_, Infallible>`. Linux's complete binding
   validation returns `Result<_, CompositionError>`. Executable hosts accept
   either typed result and fail startup instead of discarding a composition
   error.
4. The public runtime identity, config path, feature registry and future
   provider composition remain in each physical OS crate. The first real
   Windows or macOS provider replaces the absent handle only for its
   implemented facet and supplies its own provider ID, availability, execution
   lane, and evidence.
5. Configuration path selection belongs to the selected native adapter:
   - Linux uses XDG/HOME conventions;
   - Windows uses APPDATA/LOCALAPPDATA/USERPROFILE conventions;
   - macOS uses the per-user Application Support directory.
   Application storage accepts an injected path and contains no target `cfg` or
   environment convention.
6. `hardware-all` remains the required standard-release profile on every OS.
   It means “all hardware providers implemented for this OS”, not “Linux
   providers everywhere” and never a vendor SKU. The implemented Windows and
   macOS provider sets are currently empty, so the honest complete registry is
   empty. Adding their first hardware backend must also add it to that OS's
   standard runtime registry.

## Consequences

The Windows and macOS source graphs now reach a real native composition edge
without pretending that telemetry or control works. Shared application and UI
code no longer need a Linux fallback shape merely to compile an adapter.

An empty capability catalog is not a completed platform port. Target builds,
native path behavior, frontend capability gating, and every future provider
still require target-host verification. No Windows or macOS hardware support
claim follows from this ADR.

The shared channel runtime's optional bindings and lanes may later extend the
two OS adapters one facet at a time. That migration must preserve the same
observable contract: absent facets remain absent, not registered stubs.
