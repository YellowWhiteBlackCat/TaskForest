# Module ownership

Beyond the README module map, these ownership facts apply:

- `src/core/metrics/` owns domain measurements and availability joins.
- `src/core/config/sidebar.rs` owns deterministic ordering, per-device visibility
  precedence and bounded preference normalization across all frontend renderers.
- `src/core/history.rs` emits portable, delimiter-safe and case-injective series
  names. Its private external-token decoder immediately produces current
  identities; renderers and persistence writers never carry external spellings.
- `src/core/process/`, `src/core/services/`, `src/core/startup/` and
  `src/core/export/` own typed product facts.
- `src/core/alerts/` owns alert rules, active-set transitions and the bounded,
  versioned event export; event history is session-local domain state, not a
  renderer cache.
- `src/core.rs` is the owner module index and `src/lib.rs` contains only the
  core crate's own aggregate API, never a cross-layer forwarding facade;
  consumers should prefer the explicit domain module that owns each fact.
  Pure behavior belongs in nearby tests.

## Local time rules

`time::LocalTimeRules` validates bounded TZif into typed offset/DST
transitions. Variable rules carry an explicit last-transition horizon; a
projection beyond that horizon is unavailable because POSIX-footer recurrence
is not yet parsed. `LocalTimeRulesObservation` distinguishes current rules
from provider failure; its cache key holds the cheap-cloned full rule value,
so semantic equality is the only reuse and change authority.
