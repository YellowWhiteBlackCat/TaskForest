# ADR-020: Layout Governance, Single-Source Utilities, and Panic-Surface Gates

- Status: accepted
- Context: architecture-review round (compile-time design, OS-isolation, panic
  surface, performance) exposed recurring failure modes that the guidelines did
  not yet forbid; this ADR records the收口 decisions so the same mistakes cannot
  re-enter.

## Decision 1: single-source utilities

Repeated implementations were found for text matching, wall-clock reads, and
display filtering:

- `contains_ascii_ci` existed as private copies in `core/process.rs` and
  `gpui_app/processes_view/filter.rs`; process/service/startup/TUI filters each
  allocated per item with `to_ascii_lowercase`.
- `duration_since(UNIX_EPOCH)` + fallback clamps existed at 10 call sites.
- Process filtering existed as three separate implementations (GPUI processes,
  TUI processes, services/startup filters).

Rule: a utility with more than one prospective caller lives in ONE place —
`taskmanager-core` (the shared floor) — with unit tests. Every frontend imports
that owner directly; no application model facade or private copy is used.
Display filters are allowed to stay in the frontend layer but must share the
core matcher, not re-implement matching.

Concrete homes: `core/text.rs` (`contains_ascii_ci`, `cmp_ascii_ci`) and
`core/time.rs` (`unix_now_ms`, `unix_now_micros`).

Lesson recorded: promoting a helper to shared code must preserve its exact
semantics — the promotion of `contains_ascii_ci` initially lost the needle
lowercasing and silently broke case-insensitivity; the shared implementation
lowercases both sides and callers pass raw queries.

## Decision 2: layout tokens for spacing and type

The radius scale was tokenized first; spacing (`px(8.0)` ×84, `px(12.0)` ×102,
…) and type sizes (`text_size(px(13.0))` …) remained magic numbers across
`taskmanager-ui` and `gpui_app` (~450 call sites). Theme-driven values are now:

- `theme/tokens.rs` `SPACE_1..=SPACE_24` (spacing scale, skin-independent) and
  `FONT_8..=FONT_26` (type scale) constants; call sites read
  `tokens::SPACE_8` / `tokens::FONT_13`.
- Radius remains skin-bound via `tokens::card_radius(t)` etc.

Rule: gap/padding/margin and text sizes in UI code use the token constants;
`px(...)` literals are allowed only for layout contracts that are NOT theme
values (column widths, chart dimensions, table minimum widths). Colors stay
token-only (ADR-017).

## Decision 3: panic discipline is compiler-CI enforced

- `tests/logic/panic_surface_test.rs` scans ALL production `src/` trees of the
  workspace for `.unwrap(` / `.expect(` / `panic!` / `todo!` /
  `unimplemented!` / `unreachable!` after stripping comments, string literals,
  and test modules; every remaining site needs a justified entry in
  `ALLOWED_PANIC_SITES`. New panic sites fail CI unless documented.
- `tests/logic/workspace_architecture_test/dependency_firewall.rs` gained a
  reverse check: OS adapters are reachable only from `taskmanager-platform-native`.
- The gate already caught real regressions during this round (an
  `unreachable!` in the procfs probe path and a new `expect` in the projection
  cache), confirming it must stay.

Rule: never add a new production panic site; prefer typed errors, `Result`
narrowing (e.g. `ProcfsProbe` became `Result<_, FailureKind>` so downstream
matches are exhaustive without a panic arm), or graceful fallbacks.

## Decision 4: render-time projection caching

The processes row model (VisibleRow + pid order) was rebuilt per frame and per
keypress; it is now cached on `RootView` (`processes_projection`, invalidated
by `processes_generation` or any state key change) and shared by the render
path and keyboard paging. The tree mode stopped cloning every `ProcessItem`:
`ProcessNode<'a>` / `FlatTreeNode<'a>` hold references.

Rule (gpui constraint discovered here): an entity in the middle of rendering
cannot be re-updated (`cx.entity().update` panics inside render); compute
projections at the render entry (where `&mut self` is available) and pass the
result down. Keyboard paths consume the SAME cached projection so paging can
never diverge from pixels.

## Verification

- `cargo nextest run --locked --workspace --all-targets -j 4` (1786 pass)
- `cargo clippy --locked --workspace --all-targets -- -D warnings` (0)
- `cargo test --locked --doc --workspace -j 4` (22 crates ok)
- Windows target clippy 0; TUI evidence re-signed
