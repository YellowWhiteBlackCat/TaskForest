# Pages and keys

Use `Alt+1`…`Alt+6` to change pages, arrows and page keys to navigate, `Ctrl+F`
to search, `F5` to refresh, and `Ctrl+Space` to pause. `Delete` opens an
identity-frozen confirmation; only `y` after that confirmation can submit an
end-task effect.

The Applications page uses one hierarchy: Applications, Background and
Uncategorized category headers; Applications then exposes PID-less selectable
application roots before their recursive process trees, while the other
categories expand directly to process rows. Ctrl+Right expands the complete
hierarchy; Ctrl+Left collapses it. The help columns scroll their measured
wrapped rows, so all shortcuts stay reachable above the fixed close hint.
Legacy saved grouping values are normalized at import and are not exposed as
alternate modes. Terminal font size remains the terminal emulator's
responsibility; TUI config writes preserve the desktop `ui_size` token.

The CPU Performance surface has no metric selector: utilization, temperature,
frequency and power facts render together, one dominant utilization history
owns the main graph, and the per-core viewport remains reachable with
arrows/page keys when the terminal has room. Compact terminals keep the facts
and main graph and omit the optional core grid.

The GPU Performance surface follows the same fixed-fact rule: aggregate
utilization owns its only device-wide history, while temperature, frequency,
power, idle residency and memory facts remain simultaneous rows. Standard
terminals add live-engine and opt-in PMU detail; compact terminals keep a
dense primary-fact strip plus the largest possible utilization chart and omit
the secondary engine region. Only the standard engine region scrolls; the main
chart never does.

Disk source failures retain SMART status and recovery guidance even with older
readings. Battery review shows charge/power history, voltage and typed fan
readings. Startup evidence puts failed unit identities before optional chain
rows so the compact view keeps the failure group.

Health exposes `p` for active warning facts and `e` for complete event
transitions. The event review uses F2 to cycle all/activated/cleared filters,
F5 to export the complete JSON history through OSC 52, and F8 to clear the
shell-owned history. Up/Down selects whole observed groups; severity,
thresholds and identity remain visible in compact terminals, and empty reviews
keep their close and navigation controls.

System `w` opens the shared dashboard; `1`–`4` select its real time window,
arrows page complete metric groups, and `b` returns to hardware facts. The
System viewport counts physical wrapped rows, so the final continuation
remains reachable in short windows. Its ordered section projection keeps NPU
devices in the fixed Graphics & accelerators section with identity, driver,
aggregate utilization, every reported engine utilization and dedicated/shared
memory; unavailable observations remain dashes and no NPU history is invented.

Persistent review uses the application controller and app-host query worker.
Performance exposes `r` to enter/leave review, `1`/`2`/`3` to select windows
and `f` to refresh. Application History shares that reader and refresh
command; compact terminals admit complete metric cards.
`ui/pinned_actions.rs` reserves wrapped controls and a bottom safety row.
`runtime/keys/direct.rs` executes the local registry and its
reader-availability guards. History preference changes reconnect through the
native connector; renderers never seed replay rows.

Optional native setup is observed quietly at live startup. When a descriptor
is available, Settings exposes F2 to review it. View/Run/Revert/Restart use
the shared application controller; only the descriptor body scrolls, and
wrapped action hints keep Close reachable in compact terminals.

About exposes build version, license, repository opening and a distinct System
Information review. Both use shared shell projections; `information.rs` owns
frozen system rows and clipboard payloads, while the modal protocol registry
owns action keys and fixed footer hints. Only the information body scrolls.
Native boot submits desktop appearance through the shared effect seam and
caches its response.
