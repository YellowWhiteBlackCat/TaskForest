# Verification

Commands (see also the README Contract section):

```bash
cargo check --locked -p taskmanager-bevy-ui --tests
bash scripts/accept-bevy-interactions.sh
cargo clippy --locked -p taskmanager-bevy-ui --tests
python3 scripts/quality/bevy_bsn_guard.py --mode enforce   # 100% Scene law
python3 scripts/quality/bevy_world_access_guard.py         # resource access scope
bash scripts/capture-bevy.sh                               # real pixels, live Wayland
```

The capture validator is fail-closed on app_id, PID/window identity, PNG,
markers, source provenance and current worktree.

Seam and projection tests are headless in `tests/headless/`: drain bound and
idle behavior against scripted event ports, runtime-cache singleton
semantics, palette token mapping, routing/keyboard semantics and nav
highlight, virtual-table-window and chart math, page/widget scene assembly on
`MinimalPlugins`, plus the drain→summary-line wiring without a compositor.
Real-window pixel evidence belongs to the capture flow defined in
[`QUALITY_GATES`](../../../docs/QUALITY_GATES.md).

Check the shared workspace resolution keeps `bevy_app`/`bevy_ecs` at one 0.20
version across this crate and `taskmanager-platform-runtime`
(`cargo tree -p taskmanager-bevy-ui -d`).
