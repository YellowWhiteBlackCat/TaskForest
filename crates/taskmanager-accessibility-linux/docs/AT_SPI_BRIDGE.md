# AT-SPI bridge notes

`src/bridge.rs` owns the AT-SPI (D-Bus) connection and lifecycle;
`src/mapping.rs` adapts typed semantic snapshots to AccessKit/Unix roles,
states, focus and notifications.

The bridge publishes platform accessibility without owning page state or
rendering: node identity, roles, names, states, focus and notifications are
derived only from the shared ui-contract semantic snapshot.

Unavailable desktop accessibility support is typed and visible — the bridge
never fabricates an active accessibility tree.

## Verification

Keep the semantic tree aligned with the renderer projection; verify with the
crate tests and the real AT-SPI receipt described in
[`screenshots/README`](../../../docs/screenshots/README.md).

```bash
cargo nextest run -p taskmanager-accessibility-linux -j 4
```
