# ADR Index

`adr/` records irreversible decisions. Every ADR states the current decision; when a
newer ADR supersedes part of an older one, the chain is listed under "Decision chains"
below and inside the ADR itself. Read an ADR only when you need the rationale — current
rules live in `docs/`, and the one-line summaries here are enough to locate the right
ADR.

Numbering has gaps (012, 021, 030, 034 were retired during drafting). Never renumber
existing ADRs: cross-references and history depend on stable numbers.

## Index

| ADR | Title | Decision in one line | Topic |
|---|---|---|---|
| [005](005-multi-crate-functional-architecture.md) | Multi-crate functional architecture | Ports-and-adapters workspace: dependencies point inward only; core owns identity, failure and source facts; frontends share one command vocabulary. | Layering |
| [006](006-platform-and-hardware-axes.md) | Separate platform builds from hardware capability | Platform builds and hardware capability are separate axes; standard artifacts enable hardware-all; vendor features are never release SKUs. | Platform, Release |
| [007](007-capability-facets-and-provider-registries.md) | Capability facets and provider registries | Each capability is an independent facet with its own provider, channel, request and failure vocabulary; missing facets never fabricate zeros. (Snapshot-cadence note superseded by ADR-011.) | Capability, Observation |
| [008](008-platform-runtime-execution-mechanics.md) | Reusable runtime execution mechanics | Shared runtime mechanics: one bounded channel per capability, fair control/observation delivery, health catalog and assembly checks. | Runtime, Layering |
| [009](009-native-os-adapter-selection.md) | Honest compile-time native adapter selection | Compile-time selection picks exactly one native OS adapter; unimplemented platforms reuse the absent-capability handle and return typed `Unsupported`. | Platform, Layering |
| [010](010-storage-topology-smart-and-lifecycle.md) | Storage topology, SMART, lifecycle | Storage facts split into four orthogonal axes; SMART jobs are keyed by (device, generation); the block-device inventory is the only discovery authority. | Platform |
| [011](011-independent-system-observation-facets.md) | Independent system observation facets | System observation splits into six correlated capability domains; application correlation owns the join; supersedes ADR-007's cadence note (migration complete). | Platform, Capability |
| [013](013-independent-process-insight-facets.md) | Independent process insight facets | Process insight splits into network/GPU/resource/isolation capabilities; frozen identity plus before/after revalidation guards PID reuse. | Process, Platform |
| [014](014-correlated-telemetry-history-and-refresh-policy.md) | Correlated history and refresh ownership | Refresh policy belongs to the application layer; history is written only after platform-client correlation, with generation-keyed gaps. | History |
| [015](015-optional-observation-semantics.md) | Optional observation semantics | Optional observations are two orthogonal axes (semantic state × freshness); `ScalarObservation<Option<T>>` is forbidden. | Observation |
| [016](016-typed-observation-wire-invariants.md) | Typed wire payloads fail closed | Typed wire payloads reject contradictory combinations instead of normalizing them; Unknown is a compatibility state, never current truth. | Observation, Safety |
| [017](017-own-ui-component-layer.md) | Own the UI component layer | gpui-component is removed; the component layer is owned by theme/icons/ui crates with boundary tests locking zero external references. | UI |
| [018](018-windows-telemetry-safety.md) | Windows telemetry safety | Windows telemetry uses safe crates by default; native calls exist only inside the ADR-031 boundary; gaps are recorded as typed, never fabricated. | Platform, Safety |
| [019](019-macos-telemetry-safety.md) | macOS telemetry safety | macOS telemetry uses safe libraries plus bounded shell-out only; APIs without a safe source are not called. | Platform, Safety |
| [020](020-layout-governance-and-single-source.md) | Layout governance and single source | Shared utilities live in core; spacing/typography become tokens; panic sites are CI-gated; renderer projection caches are shared. | UI, Perf |
| [022](022-audited-perf-boundary-crate.md) | Audited perf_event_open boundary | perf-ioctl is an audited unsafe boundary reading i915 PMU per-engine usage; business crates remain unsafe-free. | Safety, Perf |
| [023](023-per-feature-privilege-escalation-framework.md) | Per-feature privilege escalation | taskmanager-escalation is the escalation seam: default UnprivilegedGate, one authorization per feature, mutually exclusive typed outcomes. | Privilege |
| [024](024-afpacket-boundary-crate.md) | Audited AF_PACKET boundary | afpacket is an audited boundary for per-process network attribution; without CAP_NET_RAW it degrades honestly. | Safety, Platform |
| [025](025-fd-bridge-scm-rights-boundary-crate.md) | Audited SCM_RIGHTS fd boundary | fd-bridge is an audited boundary passing privileged fds from the launcher to the unprivileged app via SCM_RIGHTS. | Safety, Privilege |
| [026](026-toolkit-neutral-theme-layer.md) | Toolkit-neutral theme layer | The theme layer is toolkit-neutral by default and the skin registry is the only color source; binding ownership now ADR-051. | UI |
| [027](027-renderer-independent-shell-state.md) | Renderer-independent shell state | taskmanager-shell owns renderer-neutral state: frontends share data folds and page identity has a single source. | Frontend, Layering |
| [028](028-iced-frontend-three-peer-frontends.md) | Iced, the third peer frontend | Iced joins as a third peer frontend reusing the shell state machine and neutral tokens. | Frontend |
| [029](029-one-binary-three-ui-shapes.md) | One binary, three UI shapes | One binary with feature-gated UI shapes and a unified CLI; app-host is the only shared native composition seam. Superseded by ADR-051. | Frontend, Release |
| [031](031-windows-native-safe-boundary.md) | Minimal Windows native boundary | windows-api is the fourth minimal audited unsafe boundary; it only returns typed values and is consumed by the Windows adapter. | Safety, Platform |
| [032](032-tray-neutral-contract-and-multi-adapter.md) | Tray-neutral contract, multi-adapter | Tray vocabulary is a neutral core contract; Linux uses ksni, Windows/macOS share tray-muda. | UI, Platform |
| [033](033-bevy-ecs-runtime-scheduling-kernel.md) | Bevy ECS scheduling kernel | Bevy ECS schedules capability work only inside platform-runtime; ports and lanes are unchanged. | Runtime |
| [035](035-windows-uac-foreign-process-control-transport.md) | Windows UAC control transport | Foreign-process control uses `ShellExecuteExW("runas")` launching the fixed helper each time with creation-token revalidation; no fifth unsafe boundary. | Privilege, Process |
| [036](036-in-process-history-persistence.md) | In-process opt-in history persistence | The enabled frontend session owns the history writer (current writer-ownership decision); opt-in, default off; no standalone collector or service. | History |
| [037](037-parallel-frontend-hosts-and-layer-shell-contract.md) | Parallel hosts and layer-shell contract | Every graphical frontend keeps parallel standalone and layer-shell host paths; the neutral surface contract lives in app-host. | Frontend, Layering |
| [038](038-gpui-frame-content-budget.md) | GPUI frame content budget | The GPUI root computes an immutable FrameBudget per frame minus real shell areas; ContentBudget drives page strategies. | Frontend, Perf |
| [039](039-performance-page-composition-root.md) | Performance page composition root | `perf_views::layout` is the single Performance composition root; ChartSpec+ChartTier define card contracts; three parallel page shells are retired. | Frontend, Perf |
| [040](040-optional-setup-discovery.md) | Optional setup discovery | Optional setup is observed in the background without modals; its entry lives in Settings; First Run only serves explicit user actions. | Capability, Frontend |
| [041](041-compositor-evidence-and-gamescope-boundary.md) | Compositor evidence, gamescope | Layer-shell acceptance requires a real desktop compositor; gamescope is only an auxiliary pixel backend, never a substitute. | Frontend |
| [042](042-page-family-split.md) | The page-family split | Data pages compose through one shared PageScaffold shell that owns the status bar; chart pages keep their own composition root. | Frontend, UI |
| [043](043-defer-openharmony-native-telemetry.md) | Defer OpenHarmony telemetry | OHOS CPU/memory telemetry is deferred as capability absence with typed `Unsupported`; no sysinfo. | Platform |
| [044](044-feature-gated-android-provider-seam.md) | Feature-gated Android seam | Android gets its own feature-gated provider seam, starting as capability absence. | Platform, Release |
| [045](045-iced-cryoglyph-fixed-lru-floor.md) | Iced cryoglyph lru floor | Vendor cryoglyph and raise the `lru` floor to 0.18.2 with a hard dependency guard (temporary for the 0.1.0 line). | Release |
| [046](046-bevy-fourth-shared-contract-shape.md) | Bevy, fourth shared-contract frontend | Bevy is the fourth frontend under the shared contract with explicit per-intent declarations; ADR-029's one-binary contract is unchanged. | Frontend |
| [047](047-owner-direct-cross-crate-boundary.md) | Owner-direct cross-crate boundary | Cross-crate facts are imported only from their owner modules (core, platform-contract); forwarding facades are forbidden. | Layering |
| [048](048-msr-read-helper.md) | MSR read helper | A forbid(unsafe) MSR helper pre-reads fixed registers, reports honest nulls and crosses via pkexec under ADR-023. | Privilege, Platform |
| [049](049-amd-msr-readouts-safe-file-io.md) | AMD MSR readouts | Extends ADR-048: family-gated AMD P-state block reads yield multiplier and Vcore; temperature and BCLK stay typed-absent. | Platform, Safety |
| [050](050-wayland-current-window-capture.md) | Wayland current-window capture | GPUI submits a typed one-shot PNG request; Linux uses fixed-argv Spectacle today, with Portal Screenshot and ScreenCast/PipeWire reserved behind the same host boundary. | Platform, Frontend |
| [051](051-four-frontend-products-zero-ui-features.md) | Four frontend products, zero UI features | Each frontend is an independent product crate + bin over the shared `taskmanager-cli` harness; `ui-*` features and every frontend conditional in shared layers are deleted; `cfg` is platform-axis only. | Frontend, Release, Layering |
| [052](052-private-capture-run-isolation.md) | UUID-scoped private capture runs | Each background capture owns a UUID-scoped D-Bus/Wayland/KWin/runtime/binary/receipt namespace, a cgroup-supervised process tree and an atomic locked publication pointer; dual-run isolation is a release gate. | Frontend, Safety, Release |

## Decision chains

- ADR-011 supersedes ADR-007's provisional statement about a cohesive system-snapshot
  cadence; ADR-007's facet/registry rules remain in force.
- ADR-036 is the current writer-ownership decision for opt-in history persistence and
  replaces the earlier standalone-collector arrangement; ADR-014's refresh-policy
  ownership still stands.
- ADR-039 is the current Performance-page composition root and retires the three
  parallel page shells.
- ADR-042 is the current data-page composition decision and retires per-page outer
  wrappers; the chart-page root stays ADR-039 and page identity stays ADR-027.
- ADR-046 keeps ADR-029's one-binary contract unchanged while adding Bevy.
- ADR-051 supersedes ADR-029's feature matrix and one-binary dispatch (the unified CLI
  survives in `taskmanager-cli`) and amends ADR-026's feature-gated-bindings clause;
  ADR-046's Bevy peer-crate model becomes the pattern for every frontend.
- ADR-045 is temporary for the 0.1.0 preparation line; remove the local `lru` floor
  when an upstream-compatible cryoglyph release exists.
- Audited-boundary family: ADR-022 → 024 → 025 extend the escalation framework of
  ADR-023; ADR-031 defines the Windows native boundary consumed by ADR-018; the
  ADR-048/049 MSR chain runs under ADR-023.
- ADR-052 governs the Linux background evidence route introduced beside ADR-041
  and ADR-050; it does not turn diagnostic receipts into parity evidence.

## Find by topic

| Topic | ADRs |
|---|---|
| Layering & cross-crate boundary | 005, 008, 009, 011, 027, 037, 047 |
| Platform acquisition & data sources | 006, 009, 010, 011, 013, 018, 019, 024, 031, 043, 044, 049, 050 |
| Observation & availability semantics | 007, 015, 016 |
| History & refresh | 014, 036 |
| Privilege & escalation | 023, 035, 048 |
| Audited unsafe boundaries | 022, 024, 025, 031 |
| Frontend & UI | 017, 020, 026, 027, 028, 029, 032, 037, 038, 039, 040, 041, 042, 046, 050, 051, 052 |
| Runtime scheduling | 008, 033 |
| Storage & persistence | 010, 036 |
| Release & packaging | 006, 028, 029, 044, 045, 051 |
