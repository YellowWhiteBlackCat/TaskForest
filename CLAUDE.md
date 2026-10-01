# CLAUDE.md

Claude Code enters this repository through this file. It is a thin host adapter: a
reading order plus canonical links. It owns no rules and duplicates no content.

## Canonical sources

The shared engineering charter and the top of the project map is [`AGENTS.md`](AGENTS.md).
On conflict with `AGENTS.md`, `docs/`, or the quality guards, the canonical source wins:
fix this file in the same change instead of forking the rule here.

- Development, build, test, and command rules: [`docs/STANDARDS.md`](docs/STANDARDS.md)
  and [`docs/QUALITY_GATES.md`](docs/QUALITY_GATES.md).
- Architecture and data direction: [`docs/ARCH.md`](docs/ARCH.md),
  [`docs/HOST_ARCHITECTURE.md`](docs/HOST_ARCHITECTURE.md),
  [`docs/STATE_OWNERSHIP.md`](docs/STATE_OWNERSHIP.md).
- Permissions, helpers, and trust boundaries: [`docs/PERMISSION_MODEL.md`](docs/PERMISSION_MODEL.md).
- Private-material policy: [`AGENTS.md`](AGENTS.md) and [`docs/README.md`](docs/README.md).

## Reading order

1. [`AGENTS.md`](AGENTS.md) — shared charter, invariants, project-map top.
2. [`docs/README.md`](docs/README.md) — task routing table; read the minimum doc set.
3. The affected `crates/*/README.md` — crate responsibilities, boundaries, module map
   (overview at [`crates/README.md`](crates/README.md)).
4. [`adr/README.md`](adr/README.md) — current irreversible decisions, read by index,
   never top-to-bottom.

The root [`README.md`](README.md) is the human-facing product introduction, not a
development guide or navigation route.

## Claude Code host notes

- Claude Code enters through this file; the canonical reading order still starts at
  `AGENTS.md`, and this file adds only host entry behavior.
- Do not copy command matrices, architecture tables, conventions, or status into this
  file; link the canonical owner instead.
