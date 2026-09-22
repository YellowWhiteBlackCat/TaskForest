# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Document hierarchy

Read before changing code — start with the task routing table in [`docs/README.md`](docs/README.md),
not by scanning the whole tree:

1. [`AGENTS.md`](AGENTS.md) — global engineering charter, invariants, authority route.
2. [`docs/README.md`](docs/README.md) — task routing table; find the minimum doc set for your task.
3. `crates/*/README.md` — crate responsibilities, boundaries, module map.
4. [`adr/README.md`](adr/README.md) — irreversible decisions index (read by index, not top-to-bottom).

The root [`README.md`](README.md) is a product introduction, not a development guide.

## Build commands

Each frontend is an independent product crate (ADR-051) with its own binary:

```bash
cargo build -p taskmanager-gpui          # GPUI desktop → target/debug/taskforest-g
cargo build -p taskmanager-iced          # Iced desktop → target/debug/taskforest-i
cargo build -p taskmanager-tui           # TUI terminal → target/debug/taskmanager-tui
cargo build -p taskmanager-bevy-ui       # Bevy (experimental) → target/debug/taskforest-b
cargo build --release -p taskmanager-gpui # release build with LTO
```

Toolchain is `stable` via `rust-toolchain.toml`. Edition is Rust 2024. Max parallelism: 4 jobs
(`-j 4`). Workspace uses shared `target/` and `.tmp/` for scratch.

## Test commands

**Non-negotiable rule**: all non-doctest tests MUST use `cargo nextest run ... -j 4`.
Bare `cargo test` (without `--doc`) is mechanically rejected by the test runner guard.

```bash
# Run all workspace tests
cargo nextest run --locked --workspace --all-targets -j 4

# Run a single crate's tests
cargo nextest run --locked -p taskmanager-core -j 4

# Run a single test by name
cargo nextest run --locked -p taskmanager-core -j 4 -E 'test(test_name_here)'

# Doctests (the ONLY case where cargo test is allowed)
cargo test --locked --doc --workspace -j 4

# Single crate doctests
cargo test --locked --doc -p taskmanager-core -j 4
```

## Quality gates (local)

```bash
bash scripts/quality/local-gates.sh quick      # fmt, policy gates, guards (~minutes)
bash scripts/quality/local-gates.sh standard   # quick + clippy + nextest + doctests + release build
bash scripts/quality/local-gates.sh extended   # standard + coverage + mutation + Miri + fuzz
```

Scope isolation for parallel frontend work:
```bash
bash scripts/quality/local-gates.sh standard --scope gpui   # only GPUI closure
bash scripts/quality/local-gates.sh standard --scope bevy   # only Bevy closure
```

Run the full local gate suite before every push.

## Lint and format

```bash
cargo fmt --all                           # format
cargo fmt --all -- --check                # check only
cargo clippy --locked --workspace --all-targets -j 4 -- -D warnings
```

## Architecture

TaskForest is a cross-platform system monitor (Linux/Windows/macOS). The dependency flow
is strictly one-way:

```
frontend → application → core/shell → platform runtime → app-host/native → OS
```

Return direction carries only typed events, snapshots, and failures:
```
OS → platform adapter → runtime → application reducer → cached projection → frontend
```

### Layer responsibilities

| Layer | Owns | Must not |
|---|---|---|
| `core` | Domain facts, availability semantics, pure rules | OS I/O, threads, UI types |
| `application` | Commands, reducers, ports, task state | Platform APIs, rendering |
| `shell` | Frontend-neutral projections, caches, interaction vocabulary | Direct collection, toolkit widgets |
| `platform-contract` | Capability traits, request/event types | Implementation |
| `platform-{linux,windows,macos}` | Platform data sources, control adapters | Redefine domain semantics |
| `platform-runtime` | Scheduling, concurrency, backpressure, event delivery | UI lifecycle ownership |
| `app-host` | Adapter selection, runtime composition | Second fact model, toolkit surfaces |
| `cli` | Shared CLI: parsing, neutral mode, help, tracing | UI lifecycle, per-frontend `cfg` |
| frontend crates | Render projections, submit commands | OS I/O, blocking collection, business rule forks |

### Four frontends, one contract

All four frontends consume the same application projections — same domain facts, same commands,
same availability semantics. They are four independent product crates, not feature-gated variants.
The UI axis uses crate composition with zero `cfg`; the platform axis uses `cfg(target_os)` only
at the composition boundary (`taskmanager-platform-native`).

### Boundary crates (the only `unsafe` code)

`unsafe` exists in exactly four audited boundary crates:
`taskmanager-perf-ioctl`, `taskmanager-afpacket`, `taskmanager-fd-bridge`, `taskmanager-windows-api`.
All other crates use `#![forbid(unsafe_code)]`. Enforced by the dependency firewall test.

## Key conventions

- **Module shape**: `foo.rs` + `foo/` — never `foo/mod.rs`.
- **No cross-crate forwarding facades**: import domain types from `taskmanager-core`, capability
  types from `taskmanager-platform-contract`. Composition crates don't re-export shared types.
- **Hard cutover**: when a new typed contract replaces an old one, delete the old API, alias,
  wrapper, fallback, renderer state, fixture, and caller in the same change.
- **No fabricated data**: unknown, real zero, temporary failure, permission denied, and unsupported
  are distinct states. Never mask collection failure with `0` or empty.
- **Unprivileged by default**: privilege escalation is per-feature, OS-native, typed. Never
  blanket `setcap`/`setuid`.
- **File size**: production Rust files hard-fail at 650 non-empty, non-comment lines; test files
  at 999 lines. Enforced by `scripts/quality/rust_line_guard.py`.
- **Test location**: `src/` is production only. Tests go in `tests/{common,headless,gui}.rs` +
  matching subdirectories. No inline `#[test]` or `mod tests {}` in production files.
- **Test temp files**: use `repo_temp_dir()` (maps to `.tmp/`), never `std::env::temp_dir()`.
- **Windows**: no PowerShell in production code, tests, or helpers. Native API only.
- **Commits**: routine work goes directly to `main` (no feature branches). Don't push unless asked.
- **Language**: Rust/code comments in English; detailed prose docs in Chinese.

## Private material

Never commit to the public tree: `.private/` content, real host screenshots, system snapshots,
TODO/roadmap, internal scores, credentials, personal email, host-specific absolute paths, or
dated audit material. Private material belongs in `.private/` (git-ignored).
