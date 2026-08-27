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
safe cross-platform `battery` crate, and the pure EDID base-block parser that
native display inventories map into their own display models. Platform
adapters still provide their provider IDs and stable identity namespaces.

Portable battery collection assembles `BatteryScalarObservations` first and
applies that group once to `BatteryInfo`; it never writes schema-v1 scalar
options, whose projection is owned exclusively by core serde.

## Contract and verification

Every operation is bounded, cancellable where its contract requires it, and
returns typed failures without fabricated facts. Verify implementations with
host-neutral behavior fixtures; native adapters separately prove composition.
The directory scanner retains its active `ReadDir` cursor across calls, charges
every cursor step to the entry-or-100-ms chunk boundary, and consumes the global
entry budget for every successful `DirEntry` before metadata interpretation.
Files, directories, symlinks, sockets, and other special entries therefore share
one bound; typed file/directory totals do not charge the same entry twice.
The command lifecycle caps each output stream and their combined bytes, kills
and reaps on timeout/reader/limit failure, and joins both readers. It accepts a
safe owned process-tree spawner seam; the Windows adapter injects its audited
suspended-create, atomic Job-assignment and `CREATE_NO_WINDOW` implementation,
so this shared crate never depends on the native boundary.
Unix callers are trusted fixed-argv tools that must not daemonize, call
`setsid`, or double-fork. The runner kills their process group, but does not
misrepresent that group as a kernel-enforced tree; cancellable nonblocking pipe
readers still enforce the original deadline if a tool violates the contract.
