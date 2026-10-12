# Shared implementations

Current shared implementations are the bounded directory scanner
(`directory_usage.rs`), the fixed-argument child-process lifecycle
(`command.rs`), battery snapshot assembly through the safe cross-platform
`starship_battery` crate (`battery.rs`), and the pure EDID base-block parser
(`edid.rs`) that native display inventories map into their own display
models.

Portable battery collection assembles `BatteryScalarObservations` first and
applies that group once to `BatteryInfo`; it never writes schema-v1 scalar
options, whose projection is owned exclusively by core serde.

## Directory scanner

The directory scanner retains its active `ReadDir` cursor across calls,
charges every cursor step to the entry-or-100-ms chunk boundary, and consumes
the global entry budget for every successful `DirEntry` before metadata
interpretation. Files, directories, symlinks, sockets, and other special
entries therefore share one bound; typed file/directory totals do not charge
the same entry twice.

## Command lifecycle

The command lifecycle caps each output stream and their combined bytes, kills
and reaps on timeout/reader/limit failure, and joins both readers. It accepts
a safe owned process-tree spawner seam; the Windows adapter injects its
audited suspended-create, atomic Job-assignment and `CREATE_NO_WINDOW`
implementation, so this shared crate never depends on the native boundary.

Unix callers are trusted fixed-argv tools that must not daemonize, call
`setsid`, or double-fork. The runner kills their process group, but does not
misrepresent that group as a kernel-enforced tree; cancellable nonblocking
pipe readers still enforce the original deadline if a tool violates the
contract.

## Verification

Verify implementations with host-neutral behavior fixtures; native adapters
separately prove composition (rule owned by the README Contract section):

```bash
cargo nextest run -p taskmanager-platform-portable -j 4
```
