# Window presentation contract

`WindowPresentation` is the toolkit-neutral request for one frontend-owned
surface. `Standalone` preserves the existing normal-window host. `LayerShell`
contains only owned values from `LayerShellSpec`: layer, anchor,
compositor-selected or explicit size, margins, exclusive zone, keyboard
interaction, output hint, namespace and fallback policy. It carries no
Wayland object, event queue, raw handle or renderer state.

The role is per surface. GPUI, Iced and Bevy may each expose a standalone
host and a layer-shell host while sharing the same application projection. A
layer-shell adapter must probe the compositor capability, validate the
configuration, and either use its own native path or return the typed
fallback; it must not claim normal-window operations such as maximize,
minimize or move when the selected surface role cannot provide them.

The current GPUI opt-in uses `LayerShellSpec::desktop_widget`: a bounded
`520×360` Top-layer surface anchored to the top-right with 16px margins and
no exclusive zone. This profile is separate from the standalone default and
from the generic top-panel constructor, so enabling the widget cannot change
normal desktop-window geometry.

See [ADR-037](../../../adr/037-parallel-frontend-hosts-and-layer-shell-contract.md)
for the parallel-frontend-host decision.
