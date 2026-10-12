# Capability parity and page-level divergence

Component/surface capability parity is MACHINE-CHECKED (CORE-08):
`src/capabilities.rs` declares every ui-contract registry capability with an
explicit decision — `Ported`, `Native`, or a reasoned `Divergent`/
`Unsupported`. The one registered divergence: Toast (transient feedback renders
through the shared footer activity line). TextSelection left the divergence
list when `src/ui/components/selectable_text.rs` ported the reference
semantics — drag/word/all selection, primary-clipboard on drag finish,
Ctrl/Cmd-C copy — and the network page's per-field copy buttons were retired
with it. A silent omission or unexplained difference fails the crate gate.

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
- The GPU metric selector stays unaligned by design (ICED-014): every measured
  family renders simultaneously, so the selection surface and its message were
  deleted outright; the shared shell selection state machine remains reachable
  through behavior tests.
