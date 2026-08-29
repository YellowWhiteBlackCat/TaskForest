# ADR-047: Owner-direct cross-crate type boundary

Status: accepted

## Context

The application crate had accumulated a model facade that forwarded core facts,
platform contracts and source-status vocabulary. Frontends and adapters then
imported the same fact through multiple public addresses. This obscured the
authority route and made a new facade easy to add accidentally.

The root package and GPUI crate also carried local `core`/`i18n` forwarding
modules, while composition crates forwarded tray and instance contract types.
Those paths were historical convenience surfaces, not independent owners.

## Decision

1. A cross-crate consumer imports a fact from its actual owner module:
   `taskmanager-core` for domain facts and `taskmanager-platform-contract` for
   capability, request, event, failure and port contracts.
2. `taskmanager-application` exposes only application-owned commands, reducers,
   lifecycles and projections. Its former model module is retired.
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
  compatibility aliases cannot silently preserve a retired facade.
- Frontend crates carry direct `taskmanager-core` and, where needed,
  `taskmanager-platform-contract` edges. Their write sets remain independent.
- Composition code still owns OS selection and lifecycle wiring, but shared
  vocabulary cannot drift through a second public address.

## Verification

The workspace dependency firewall records the direct owner edges. Its frontend
checks reject cross-layer forwarding re-exports and reject native adapter
imports. The workspace all-target check and the scoped crate gates verify that
the migration preserves behavior and contract coverage.
