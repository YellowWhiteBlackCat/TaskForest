# ADR-054: Terminal semantics and GUI native accessibility receipts

Status: accepted

## Context

All four frontends share toolkit-neutral accessibility semantics, but they do
not share a native accessibility tree. A TUI runs inside a terminal emulator;
on Linux, Orca reaches the terminal emulator's AT-SPI tree. A TUI semantic
snapshot can describe TaskForest rows, but TaskForest does not own or register
that native tree. Graphical frontends publish through toolkit and platform
accessibility adapters. A validated snapshot, a headless adapter, or an
adapter reporting `Ready` does not show that a native assistive-technology
consumer can observe the application.

## Decision

1. `taskmanager-ui-contract` owns the toolkit-neutral semantic snapshot and
   semantic row inputs, including process rows and structural group rows.
   Consumers import these types from their owner modules; application, shell,
   app-host, and frontend facades do not re-export them.
2. Frontends derive semantic rows from the same in-scope visible projection
   used by their renderer. Row identity, order, structural role, selection,
   expansion, and typed values follow that shared input. Structural groups
   remain groups and are never represented as fabricated processes. This
   preserves the existing CORE-02 Applications row scope; it does not add
   process-row coverage for Services, Startup, or Users.
3. TUI publishes truthful terminal semantics from the shared contract. Its
   native screen-reader tree belongs to the terminal emulator, so TaskForest-T
   does not register an application AT-SPI tree or claim a native receipt from
   a semantic snapshot. Terminal presentation and accessibility behavior may
   differ from a GUI while preserving the shared row facts and identities.
4. GPUI, Iced, and Bevy own their toolkit and platform adapters for publishing
   native accessibility trees. Their native mappings may differ, but they
   consume the shared semantic inputs and preserve the declared row meaning.
5. A GUI native accessibility receipt is scoped to one frontend, target OS,
   native accessibility backend, and real assistive-technology consumer. It
   must show that the consumer can observe the published tree and its in-scope
   row identities, order, roles, values, and supported actions. A headless
   snapshot or mapping test is useful contract evidence but is not a native
   receipt. Bridge `Ready` means that a linked adapter can accept publication;
   it does not assert that an assistive technology is attached. `Detached` or
   another unavailable status is an honest absence and never a receipt.
6. Toolkit, OS accessibility API, and native node types stay in the owning
   frontend or platform adapter. The shared contract carries only semantic
   values and actions.

## Consequences

- Semantic parity is shared across terminal and GUI products; native receipt
  evidence follows the actual owner of each platform accessibility tree.
- TUI is not blocked on an app-owned AT-SPI bridge. Its semantic rows remain
  aligned to the visible Applications projection, while a terminal-emulator
  accessibility claim requires evidence from that emulator path.
- GPUI, Iced, and Bevy may claim native support only for frontend/platform
  combinations backed by a real consumer receipt. A linked bridge, headless
  test, or detached projection does not establish that claim.

## Verification

Shared contract tests verify snapshot validity and typed row semantics.
Frontend behavior tests verify that semantic rows track the in-scope visible
projection and that supported actions use the published revision. Each GUI
native-support claim additionally requires a live receipt from its target
platform and assistive-technology consumer; no headless result substitutes.
