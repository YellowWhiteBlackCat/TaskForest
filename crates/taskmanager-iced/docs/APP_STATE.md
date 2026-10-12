# App state and input scope

- `src/app/surface.rs` owns the single Iced-local primary surface, the single
  context-menu slot, branch-matched close transitions and the derived
  `InputScope`. Shared confirmations and Process Properties remain in the
  application `InteractionState`; payload models such as Run Task and Alert
  Center do not own parallel visibility booleans.
- Focus and motion consume `PresenceTransition::{StableClosed, Opened,
  StableOpen, Closed}` derived from before/after scopes, never a pair of modal
  visibility booleans.
- `src/app/` owns navigation, focus, settings and projection state;
  `src/app/update/` has one exhaustive message→domain authority. The main
  update entry only captures its prelude, dispatches once, then runs the common
  finish systems; input, navigation, service, control, surface, performance,
  transfer, Alerts and window reducers return one typed effect/task envelope
  and cannot bypass lifecycle convergence.
- The frontend-only Alerts page is `FrontendRoute::AlertsPage`, mutually
  exclusive with `SharedPage`; it is not an `open: bool` beside the shell page.
  Selecting a shared page or explicit close returns to `SharedPage`, bare Esc
  closes Alerts only when no higher-priority surface owns input, and a modal
  closes first while preserving the route underneath it.
- The Alerts page reads the shell's immutable canonical `ManagedAlertRule`
  list. Wrapped cards expose creation,
  metric/threshold/duration/hysteresis/severity/target edits, removal and
  clipboard transfers through shared edits. Fixed actions surround one bounded
  scroll body; stale controls resolve stable rule identities against current
  state.
- System Health selects shared filesystem, sensor and generation-matched SMART
  facts in a bounded body. Self-test controls carry the painted physical
  identity; an inventory replacement invalidates their activation.
  `AlertsPageState` contains only its typed route; disabled rules are neither
  copied nor removed from the list.
- `src/app/config_sync.rs` is the only configuration bridge: production
  receives an app-host `ConfigClient`, ticks drain immutable revision
  publications, and settings use non-blocking base-aware submissions. Queue
  rejection/save failure rolls renderer preferences back to the canonical
  snapshot without changing page, focus, scroll or runtime state; demo/default
  instances remain local-only.
- `src/app/configuration_state.rs` privately owns the coordinator cursor,
  applied revision, canonical draft, immutable presentation preferences,
  language and resolved Theme. One snapshot application replaces all five
  together; there are no independent draft/preference/theme writers.
- `src/app/sidebar.rs` resolves concrete stable device keys through the core
  sidebar rules. Both rail layouts use its ordered visibility projection;
  native editor messages submit the same persisted fields through the
  coordinator. `src/ui/sidebar_editor.rs` keeps title/Close fixed around one
  bounded list and rejects stale or closed controls.
- The named input, process-presentation, performance, capture and window-time
  components under `src/app/` split renderer-only ownership out of `IcedApp`.
  Window time is tick-injected and per-window; these components contain no
  shared request payload or provider fact.
