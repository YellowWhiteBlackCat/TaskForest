# Selection matrix

`src/instance.rs` performs the compile-time selection of the one native OS
adapter (ADR-009); `src/tray.rs` selects the tray adapter (Linux uses ksni via
the neutral contract, ADR-032); `src/lib.rs` owns the executable composition
boundary and native capture selection.

| Concern | Linux | Windows | macOS |
|---|---|---|---|
| Native adapter | validated local adapter | adapter composition | adapter composition |
| Local-time rules | validated rules | typed `Unsupported` | typed `Unsupported` |
| Window capture | KDE Spectacle | Windows.Graphics.Capture | typed `Unsupported` without a renderer hook |

The selector never substitutes UTC for an unavailable local zone, and capture
never falls back to a display-wide or fabricated image. Every target must
expose the same typed platform surface with honest fallbacks; shared contract
types remain owned by `taskmanager-platform-contract` and are not forwarded
through this selector. At runtime the app-host worker invokes the registered
in-process renderer hook first, then the adapter selected here; the capture
receipt vocabulary is owned by `taskmanager-platform-contract`.

OS-specific process liveness and native path knowledge remain in the selected
adapter; the application host receives only safe probes and owned paths.

## Verification

Verify cfg edges, feature closure and reverse dependency firewalls whenever
an adapter or composition dependency changes:

```bash
cargo nextest run -p taskmanager-platform-native -j 4
```
