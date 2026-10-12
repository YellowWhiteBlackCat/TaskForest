# taskmanager-platform-provider

## Role

Platform-neutral provider SPI, capability descriptors, registration and typed
provider identity.

## Boundary

The SPI describes observation/control behavior but does not choose an OS,
perform I/O, own worker threads or render a result.

## Contract and verification

Registration discipline (one provider, one route, one bounded lane per
capability) and verification are specified in
[`docs/SPI_DISCIPLINE.md`](docs/SPI_DISCIPLINE.md).

## Module map

```text
src/system.rs  process.rs  storage.rs   one SPI module per capability domain
src/sensor.rs  service.rs  power.rs     (facts, failures, provenance; no OS choice)
src/environment.rs  integration.rs
```

Platform adapters implement these traits; semantics never change per platform.

## Docs

- [`docs/SPI_DISCIPLINE.md`](docs/SPI_DISCIPLINE.md) — per-domain module ownership and
  registration rules.
- [ADR-007](../../adr/007-capability-facets-and-provider-registries.md) — capability facets
  and provider registries.
