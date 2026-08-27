# Claude Code adapter

Claude Code follows the repository's three-layer document architecture:

1. Read [`AGENTS.md`](AGENTS.md) for the global engineering charter.
2. Read [`docs/README.md`](docs/README.md) and the affected category charter.
3. Read the affected `crates/*/README.md` before changing a crate.

The root [`README.md`](README.md) is a product introduction, not a development guide. Do not
add parallel instruction systems. Keep current prose concise and declarative. Private history,
host receipts, screenshots, scores, TODOs, and dated audit material belong outside the public
tree under `.private/`; preserve dirty worktree changes and report validation limits honestly.
Rust/code comments remain English; detailed prose docs remain Chinese.
