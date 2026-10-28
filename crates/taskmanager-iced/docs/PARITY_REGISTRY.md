# Capability parity and page-level divergence

Component/surface capability parity is MACHINE-CHECKED (CORE-08):
`src/capabilities.rs` declares every ui-contract registry capability with an
explicit decision — `Ported`, `Native`, or a reasoned `Divergent`/
`Unsupported`. The registered divergences are Tooltip, Toast and TextSelection:

- Tooltip: the focused Performance rail readout opens on keyboard focus and
  dismisses on pointer click/Escape; footer and row hint badges remain
  hover-only because their triggers are not focusable.
- Toast: transient feedback renders through the shared footer activity line.
- TextSelection: pointer/word/block selection, multi-line spans, Ctrl/Cmd-C,
  and Ctrl/Cmd-A on the readout that owns the window selection are ported; an
  unowned readout keeps Ctrl-A on the shared row-summary command because
  readouts are not keyboard-focusable.

Scrollbar is declared `Native` via iced's scrollable; every other capability is
`Ported`. A silent omission or unexplained difference fails the crate gate.

<!-- iced-capability-divergences: tooltip, toast, text-selection -->

Below is the complementary PAGE-LEVEL registry: Performance-page skin
divergences hanging on a shared semantic with a real Iced-vs-GPUI architecture
driver (ARCH.md §8.2). They are presentation executions of shared semantics,
not component-capability differences, so they stay prose here; anything
duplicating GPUI semantics without such a driver was removed or aligned
instead.

- CPU-page trend strip: the device rail's sparklines are unreachable in Strip
  frames (narrow width or F9); the strip keeps the five shared family trends
  visible.
- Inline 60/120/300s graph-points pills: the same shared persisted preference
  GPUI's Settings page owns; Iced's full-screen settings modal makes a local
  access surface the density-appropriate execution.
- Gauges row (CPU/Memory): a skin/density execution of the same
  used-percentage observations; the Memory page has no GPUI readout-band
  equivalent to carry them.
- Wi-Fi status card (network): skin-level presentation (status dot +
  signal-quality bar) of the same wireless facts the stats rail carries.
- Earlier/now chart-axis labels: a skin-level annotation of the shared
  chronological contract (oldest-left, newest-right).
- CPU+Memory dual-series chart: density execution over the same shared history
  store; the Memory page still carries its own swap headline chart so the
  shared swap-over-time semantic stays covered.
- VRAM meters in the left column and the engine-escalation toggle below the
  card: placement differences for facts GPUI pins in its stats-rail footer;
  the facts, gating, and honesty rules are identical.
- The GPU page follows GPUI's no-selector parity: every measured family renders
  simultaneously and the shared shell selection state machine stays available
  to the frontends that expose a selector (TUI's keyboard cycle, Bevy's metric
  selector). Iced has no metric-selection control, so its P0-MC-04 anchor is
  the all-families GPU page, not the shell selection seam.
