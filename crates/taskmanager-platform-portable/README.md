# taskmanager-platform-portable

## Role

Concrete, bounded safe-I/O provider implementations reused by at least two
native operating-system adapters.

## Boundary

This crate may implement the platform provider SPI and the platform-neutral
lifecycle of fixed-argument child processes. It does not choose native tools,
parse platform output, discover an OS, interpret capabilities, schedule work,
own product policy/frontend state, or collect miscellaneous helpers. A
one-platform implementation stays in its native adapter until a second real
consumer proves portability.

Current shared implementations are the bounded directory scanner, the
fixed-argument child-process lifecycle, battery snapshot assembly through the
safe cross-platform `starship_battery` crate, and the pure EDID base-block
parser that native display inventories map into their own display models.
Platform adapters still provide their provider IDs and stable identity namespaces.
Implementation-level budgets and lifecycle rules live in
[`docs/SHARED_IMPLEMENTATIONS.md`](docs/SHARED_IMPLEMENTATIONS.md).

## Contract and verification

Every operation is bounded, cancellable where its contract requires it, and
returns typed failures without fabricated facts. Verify implementations with
host-neutral behavior fixtures; native adapters separately prove composition.

## Module map

```text
src/battery.rs  command.rs  directory_usage.rs  edid.rs
```

Truly cross-platform, semantically consistent sources; platform adapters may reuse them.

## Docs

- [`docs/SHARED_IMPLEMENTATIONS.md`](docs/SHARED_IMPLEMENTATIONS.md) — scanner/command/battery
  budgets and the spawner seam.
