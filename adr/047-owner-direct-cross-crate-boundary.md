# ADR-047: Owner-direct cross-crate type boundary

Status: accepted

## Context

A cross-crate fact has one public address: its owner module. A model facade that
forwards core facts, platform contracts and source-status vocabulary gives
frontends and adapters multiple public addresses for the same fact, obscures
the authority route and allows another facade to be added accidentally.

The root package and GPUI crate must not expose local `core`/`i18n` forwarding
modules, and composition crates must not re-export tray or instance contract
types. The owner-direct rule excludes local forwarding modules and re-exports;
the owner module is the only address.

## Decision

1. A cross-crate consumer imports a fact from its actual owner module:
   `taskforest-core` for domain facts and `taskforest-contract` for
   capability, request, event, failure and port contracts.
2. `taskmanager-application` exposes only application-owned commands, reducers,
   lifecycles and projections. A cross-layer model facade is forbidden.
3. UI crates may depend directly on the two owner crates. They must not create
   a local `core` module or re-export an owner type under a frontend path.
4. `taskmanager-app-host` may expose composition functions, but it does not
   re-export tray, instance, core or platform-contract types. The native
   selector may select an OS runtime; it does not become a type facade.
5. Same-owner module indexes inside an owner crate remain ordinary public API.
   This decision forbids cross-layer forwarding, not the owner crate's own
   named module organization.

## Consequences

- Type authority is visible at every import site and dependency edges reflect
  actual use.
- Removing or changing an owner type has one compiler-visible consumer set;
  compatibility aliases cannot create a second public address.
- Frontend crates carry direct `taskforest-core` and, where needed,
  `taskforest-contract` edges. Their write sets remain independent.
- Composition code still owns OS selection and lifecycle wiring, but shared
  vocabulary cannot drift through a second public address.

## Verification

The workspace dependency firewall records the direct owner edges. Its frontend
checks reject cross-layer forwarding re-exports and reject native adapter
imports. The workspace all-target check and the scoped crate gates verify the
owner-direct boundary and its contract coverage.
