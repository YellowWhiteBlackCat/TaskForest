# ADR-060: Publisher namespace cutover to `taskforest-*` and the git-revision semver baseline

Status: accepted (implements the AGENTS.md rule that publish namespaces reserve `taskforest-*`; depends on ADR-047 owner-direct imports and ADR-055's daemon/IPC boundary)

## Context

The workspace published nothing, yet four crates were already the intended
public library surface: the platform-neutral domain core, the platform
contract, the daemon IPC protocol, and the daemon service. They carried the
internal `taskmanager-*` prefix, which is the application and helper prefix —
not the namespace the charter reserves for published libraries.

Two consequences forced a decision:

1. **Identity.** A published crate name is a permanent, unrevocable public
   promise. Publishing `taskmanager-core` first and renaming later would burn
   the old name forever and split downstream consumers across two identities
   for the same fact owner. The cutover therefore has to happen before the
   first `cargo publish`, and it has to be atomic: no forwarding facade, no
   alias crate, no dual-published prefix (AGENTS.md hard-cutover rule).

2. **The API-compatibility gate's baseline.** `scripts/quality/semver-api-gate.sh`
   compares each publishable crate against a **git revision**
   (`--baseline-rev`), because there is no crates.io release to compare
   against. It matches by *package name*, so a rename makes the renamed crate
   invisible in the baseline: it is reported as a new crate and skipped.

The gate also fails closed on vacuity — a run that compares nothing is a
failure, not a green run — precisely so that a rename cannot silently turn the
gate into a rubber stamp.

## Decision

1. **Publish namespace.** Published library crates take the reserved
   `taskforest-*` namespace. The application, frontend, helper, and test-support
   crates keep `taskmanager-*`; the two prefixes are disjoint and each has one
   meaning. Both prefixes are recognized as "workspace crate" by the dependency
   firewall (`tests/logic/workspace_architecture_test/dependency_firewall.rs`),
   which must never fall back to "unknown prefix means no dependency".

2. **Atomic hard cutover, one commit.** `taskmanager-core` →
   `taskforest-core`, `taskmanager-platform-contract` → `taskforest-contract`,
   `taskmanager-ipc` → `taskforest-ipc`, and `taskmanager-daemon` →
   `taskforest-daemon` move together: directories, `[package] name`, every
   `use` path, every manifest edge, the lockfile, the guards, the packaging
   invocations, and the public documentation. No alias, wrapper, or
   compatibility crate is left behind. Advisory binary names
   (`taskforest-g`, `taskforest-i`, `taskforest-b`, `taskforest-daemon`) are
   unchanged, because ADR-051/055 already fixed them.

3. **Baseline policy while nothing is published.** The baseline stays a git
   revision. A renamed crate that is absent from the baseline revision is
   reported `SKIP` (fresh publication, no earlier API), while the crates that
   kept their names are still compared — so the vacuity guard stays satisfied
   and the gate keeps its teeth for every crate that has a history.

4. **Baseline policy once the crates are published.** The first published
   release of each `taskforest-*` crate *is* its baseline. From that point the
   publishable set is compared against crates.io (`check-release` default
   behaviour) instead of a git revision, and the git-revision baseline is
   retained only as the pre-publication fallback for a crate that has no
   release yet.

5. **The baseline side must be locked too.** The scanning tool resolves the
   baseline crate in a placeholder workspace of its own. Where the tool exposes
   `--locked` this is a non-issue; the pinned release of the tool does not, so
   the baseline-side dependency graph is not pinned by this repository's
   `Cargo.lock`. The gate therefore reports that fact instead of implying a
   guarantee it cannot make, and a lockfile-pinned rustdoc-JSON flow is the
   recorded follow-up (see Consequences).

## Consequences

- A forthcoming major-version or API break is caught against a real baseline
  for every crate that already has one; the four renamed crates start their
  history at their first release instead of silently inheriting the old name's
  history.
- The rename is a build-breaker for any out-of-tree consumer of the old
  `taskmanager-{core,platform-contract,ipc,daemon}` names. That is accepted:
  nothing was ever published from this repository, so the only consumers are
  in-tree.
- `docs/QUALITY_GATES.md` § 2 records the lockfile exception on the scanning
  side; that exception is removed when the gate gains a lockfile-pinned
  rustdoc-JSON flow.
- Publishing is ordered by dependency: `taskforest-core` first, then
  `taskforest-contract` / `taskforest-ipc`, then `taskforest-daemon`. A
  `cargo publish --dry-run` for a dependent crate fails until its dependency is
  actually published, and that failure is a sequencing fact, not a defect.
