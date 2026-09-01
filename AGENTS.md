# Repository Strategic Charter

TaskForest is a Rust 2024 system monitor for Linux, Windows, and macOS. This file
defines repository-wide engineering boundaries; implementation detail belongs to
the lower document layers.

## Public documentation contract

- `README.md` is the product introduction: identity, capabilities, status, usage,
  platforms, releases, and license.
- `AGENTS.md` defines the global mission, boundaries, invariants, and document route.
- `docs/` contains concise current-state charters; every living document is ≤200 lines.
- `crates/*/README.md` owns crate responsibilities, contracts, dependencies, and checks.
- `adr/` records current irreversible decisions.
- History, plans, scores, TODOs, dated receipts, host snapshots, and real captures are
  private material. They belong only in ignored `.private/` storage and must never be
  committed to the public repository.

## Mission and architecture

- Linux, Windows, and macOS share one typed product contract. GPUI, Iced, TUI, and
  Bevy consume the same application projections; GPUI is the current release surface.
- Preserve the one-way flow: frontend → application → core/shell → platform runtime →
  app-host/native composition → OS.
- `core` owns typed facts and pure rules; `application` owns commands, reducers, and
  ports; platform crates own I/O; frontends render projections; composition selects
  adapters.
- One fact has one authority. A lower layer may expand an upper layer, never redefine it.
- Cross-crate forwarding facades and type re-exports are forbidden. Consumers import the
  actual owner module (`taskmanager-core` or `taskmanager-platform-contract`); composition
  crates select adapters and expose behavior, not a second public address for shared types.
- Core evolution is a hard cutover: once a new typed contract is current, delete the old
  API, alias, wrapper, fallback, renderer state, fixture, demo/capture path, and caller in
  the same change. Do not add `deprecated`, compatibility facades, dual-stack state, or a
  migration wait-list. A private external-payload decoder may canonicalize an already
  published format at ingress, but that decoder is not an implementation API or a second
  semantic path.
- Import owner types with `use` at the module boundary. Do not introduce long qualified
  paths as a substitute for an import, especially after a core type migration.

## Non-negotiable invariants

- Business crates are safe Rust. `unsafe` exists only in the four audited boundary
  crates and crosses their APIs only as typed, owned values.
- The application is unprivileged by default. Escalation is per feature, OS-native,
  typed, and observable; unavailable data is never fabricated as zero or success.
- Frontends never read OS sources directly. Blocking collection stays off the UI thread;
  rendering and keyboard paths consume the same cached projection.
- Conditional compilation is a platform-axis mechanism only (ADR-051). Each frontend is
  an independent product crate with its own binary over the shared `taskmanager-cli`
  harness; the workspace carries no `ui-*` features, and shared layers never gate code
  on a frontend identity.
- Use the owned theme/component layer, toolkit-neutral contracts, typed layout tokens,
  and the `foo.rs` + `foo/` module shape; do not add `foo/mod.rs`.
- Windows telemetry, tests, and helpers never use PowerShell or another command interpreter.
- Production code is panic-free by gate. Tests prove behavior and side effects, not source
  text, vacuous assertions, or host-specific values.
- Public-repository checks reject private paths, live captures, personal email addresses,
  credentials, and host-specific absolute paths.

## Authority route

- Start at [docs/README.md](docs/README.md), then read the relevant current charter;
  task-type routing and document layering live there.
- Read [docs/ARCH.md](docs/ARCH.md), [docs/STATE_OWNERSHIP.md](docs/STATE_OWNERSHIP.md),
  [docs/STANDARDS.md](docs/STANDARDS.md), [docs/QUALITY_GATES.md](docs/QUALITY_GATES.md);
  house terminology is defined in [docs/GLOSSARY.md](docs/GLOSSARY.md).
- For responsive or dense UI layout work, follow [docs/ELASTIC_LAYOUT_PLAYBOOK.md](docs/ELASTIC_LAYOUT_PLAYBOOK.md) and the affected UI/component charter before editing the renderer.
- Read [docs/PERMISSION_MODEL.md](docs/PERMISSION_MODEL.md), the affected crate README,
  and relevant [ADR](adr/) via the [adr/README.md](adr/README.md) index, never the
  directory top-to-bottom, before changing trust or platform boundaries.

## Working protocol

- Preserve unrelated work. Cargo uses `.tmp/`, shared `target/`, and at most four jobs; tests use `cargo nextest ... -j 4` (doctests use `cargo test --doc ... -j 4`), enforced by the quick gate.
- Routine work may proceed on `main` until the owner rescinds mainline mode.
- Before completion, run the quick gate and report pass/fail/skip with relevant evidence.
- Every visible layout change must complete the elastic-layout playbook: derive slot budgets, admit
  lower content as whole groups, protect bottom/right edges, and run headless bounds plus real capture.
- Keep commits focused. Never publish `.private/`, generated host receipts, or live captures.
