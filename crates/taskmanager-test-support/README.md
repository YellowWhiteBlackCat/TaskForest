# taskmanager-test-support

## Role

Dev-only typed fixture assembly for cross-crate behavior tests. The crate
depends on `taskforest-core` and exposes builders that write canonical
observations through named domain assembly. It is never a product dependency.
`pin_english()` pins the shared i18n global to English for tests that assert
catalog strings — the app correctly follows the host locale, so such tests
must not; every consumer depends on this crate only through dev-dependencies.

`src/setup_script.rs` supplies an injected, quiet application client with recorded typed
setup submissions. It performs no host observation or native action.

## Boundary

Schema-v1 field names, sentinel hydration, OS I/O, and source-text assertions
do not belong here. Old payload compatibility is tested directly against the
private serde boundaries in the owning domain crate.

Fixture values are composed per consumer: builders expose named setters, not
frozen canonical snapshots (`rich_memory()`-style value presets are rejected
— every real consumer asserts its own distinct values, so a preset would only
create an unused parameter surface). A shared value spec may be added only
when a consumer demonstrably needs value-agnostic rich fixtures.

## Contract and verification

Builder staging, composition discipline and the builder inventory are
specified in [`docs/BUILDER_RULES.md`](docs/BUILDER_RULES.md); check with
`cargo check -p taskmanager-test-support --all-targets` and its downstream
behavior tests.

## Module map

```text
src/lib.rs                     dev-only typed fixture builders
src/memory.rs  metrics.rs  process.rs
src/setup_script.rs  unix_socket.rs quiet setup submission and unix socket test helpers
src/desktop_appearance.rs       responding appearance port and submission recorder
src/smbios_memory.rs            responding memory-inventory port and recorded request identity
```

Consumed only through dev-dependencies; never a product dependency.

## Docs

- [`docs/BUILDER_RULES.md`](docs/BUILDER_RULES.md) — compile-time staging, composition
  discipline and the builder inventory.
