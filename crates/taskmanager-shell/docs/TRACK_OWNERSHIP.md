# Frontend track ownership

- Each frontend track owns exactly one application dependency lifecycle:
  `ShellApp` for Iced/TUI/Bevy and `DirectTrackState` for GPUI. `OpenServiceLog`
  owns only the feed plus its lifecycle; identity is projected from the
  lifecycle. Effect dispatch begins a typed admission attempt before submission
  and upgrades or rejects that exact attempt.
- Each track caches the read-only runtime capability snapshot through one named
  reducer. GPU-engine presentation combines that status with the application
  request session in one shared fold; the session is the sole accepted payload
  authority, and renderers retain no enable bool or failure-detail mirror.
- The same tracks privately own one `RequestSessions` component for affinity
  reads, process batches, SMART self-tests, GPU-engine acquisition,
  process-network escalation and command/reveal/URL actions. The shared fold
  emits correlated typed terminals once; this component filters wrong-target,
  wrong-capability, replaced, closed, late and duplicate terminals before
  renderer history, feedback, payload projection or invalidation can observe
  them.
- Each track also privately owns exactly one `SystemProjectionStore`. Renderers
  use `projection()` only; there is no `data_mut`, `projection_mut`, or public
  writable field. Production facts enter through `apply_platform_batch` and
  named control/alert reducers. `fixture::ProjectionSeedFact` and its
  domain-scoped editors are the deterministic demo/capture/test seam and never
  expose the store itself.
- The store's `AlertCenter` is the sole full managed-rule authority on each
  track. `ShellApp` and `DirectTrackState` expose only immutable managed rules
  and the same typed edit command; frontends cannot replace engine rules or
  persist enabled-state mirrors.
- The same `AlertCenter` also owns bounded activated/cleared event history;
  GPUI's event panel and Iced's event overlay consume its immutable slice,
  while filter/read state remains renderer-local.
- Effect submission folds through `NoSubmission | Rejected | Accepted`; partial
  multi-facet success remains accepted while every rejected facet is reported.
- Inline input ownership is one `ShellInputMode::{Content, Search, Help,
  Suggestions}` machine. Frontends cannot construct parallel open states.
- Shared dangerous gates consume application `InteractionState`; parallel
  `pending_*` mirrors are forbidden. The GPUI direct track owns that same
  machine inside `DirectTrackState`; its renderer-local window surface never
  stores shared semantic payloads.
