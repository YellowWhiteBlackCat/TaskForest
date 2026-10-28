# GPUI Kit patch

This is GPUI Kit 0.8.1 from crates.io. Only `src/`, `README.md`, and the
crate license are vendored; every GPUI Kit source file is byte-identical to
upstream. Two things differ from the published package:

1. **Manifest**: `x11` is removed from the `gpui-pre-platform` and optional
   `gpui-fast-platform` feature lists. `wayland` stays selected, so every
   TaskForest GPUI platform path is Wayland-only. Cargo features are additive,
   so a downstream crate cannot subtract this flag; the vendored manifest is the
   only place it can be turned off.
2. **Vendoring scope**: the upstream `tests/` tree and its three `[[test]]`
   target declarations are not carried. A patched dependency never builds its
   own test targets, and those upstream documents reference a bare
   `cargo test` invocation that this repository's test-runner guard forbids in
   tracked files.
