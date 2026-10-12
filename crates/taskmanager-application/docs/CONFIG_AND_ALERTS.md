# Configuration and alert rules

## Configuration

- `src/config_store.rs` owns base-aware configuration transactions. Each
  writer merges only top-level fields changed from its last locally observed
  snapshot into the lock-protected current disk value; an unchanged periodic
  snapshot cannot revert another writer. A background reservation freezes that
  snapshot's merge base before executor handoff; successful save advances the
  writer base to the local snapshot. An OS file lock serializes independent
  instances and processes with a bounded typed timeout.
- `src/config_runtime.rs` owns the toolkit-neutral bounded configuration
  worker, client-local base→local patch submissions, monotonic
  `ConfigRevision`, immutable full-snapshot publications, typed
  recovery/failure, slow-client resync and independent last-handle shutdown. A
  broken refresh retains the last-good snapshot; exact and lock-level no-ops do
  not publish. Its monotonic immutable publication is the single process-wide
  settings authority; clients retain only a revisioned snapshot and submit
  base→local changes back to that coordinator.

## Alert rules

- `src/managed_alert_rules.rs` owns the complete `ManagedAlertRule` list and
  typed toggle/add/update/remove/import reducer. `AlertCenter` derives its
  evaluator from enabled entries only; disabled entries remain canonical and
  visible, while missing stable identities and invalid atomic imports cannot
  mutate state. Its bounded alert-transition history is the single event-center
  source for every renderer; clear/export operate on that typed source.
- `src/alert_suggestion_window.rs` retains only the bounded evidence windows
  used by alert and SMART suggestions; general live chart history belongs to
  `taskmanager-telemetry-store`.
