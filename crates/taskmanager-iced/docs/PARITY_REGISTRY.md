# Capability parity and page-level divergence

Component/surface capability parity is MACHINE-CHECKED (CORE-08):
`src/capabilities.rs` declares every ui-contract registry capability with an
explicit decision — `Ported`, `Native`, or a reasoned `Divergent`/
`Unsupported`. The only remaining Iced divergence is TextSelection:

- TextSelection: pointer/word/block selection, multi-line spans, Ctrl/Cmd-C,
  and Ctrl/Cmd-A on the readout that owns the window selection are ported; an
  unowned readout keeps Ctrl-A on the shared row-summary command because
  readouts are not keyboard-focusable.

Scrollbar is declared `Native` via iced's scrollable; every other capability is
`Ported`. The audit compared GPUI and Iced frontend guides, this registry, and
`ComponentCapability::ALL`: the shared 19-entry behavior inventory has no
missing Iced capability. Buttons, icon buttons, pills, labels, spinners and
progress are renderer components rather than extra capability entries;
Iced's multiline Run Task editor is a local extension over the shared
single-line TextInput contract.

Tooltip now follows keyboard-visible focus on any focusable descendant by
reading the real Iced focus operation state. Non-focusable footer badges and
row-detail hints remain hover-only because they have no keyboard focus target.
This closes the tooltip gap without adding a second shared semantic or
inventing renderer-neutral state.

Toast is a real non-modal Card-glass card over the root view: the shared shell
`FeedbackPresentation` owns the copy, tone and lifetime, while Iced owns only
the floating presentation, the shared `content_button` dismiss stop and the
local dismissal receipt. Its status icon and tint resolve the shell's
`FeedbackSeverity` through the shared `StatusKind`/`StatusColors` owner and
the shared `taskmanager_icons::status_icon` glyph before native rendering —
no local severity→ink or severity→icon fold survives in this frontend.
Only `FeedbackLifecycle::Timed` expires a toast;
`UntilReplaced` and `PlatformBatches` remain visible until the shared shell
replaces or clears the notice. The footer activity line remains a durable
parallel readout of the same shell fact, not the toast's semantic replacement.

The control family (button, icon button, pill, checkbox, label, section
header, toolbar, spinner, single-line input, multi-line input, switch, slider,
select, search field, segmented track, progress) paints the SAME material the
GPUI component layer wears: the shared Card glass gradient, its specular edge,
and the theme's own two-layer glass shadow folded into the single quad Iced can
paint (`theme::control_glass`). `theme/control_measurement.rs` binds the live
theme snapshot to `ControlRole`/`resolve_size`; hover/pressed/focus/disabled
states reweight paint and never change geometry. The focus ring reads the
shared ring token, so pointer-driven focus paints no ring, and all copy is
resolved through `taskmanager-application::i18n`. `Checkbox` is now a real
`components::check_box` production path in the Applications column menu;
`TextInput` is mounted through `plain_text_input` on the Applications search
field, the Services filter and the process-properties environment filter.
`TextEditor` is mounted through `multi_line_text_input` on the Run Task
command field: its typed edit actions stay in the frontend's command draft,
and Ctrl/Cmd-Enter publishes the existing `SubmitRunTask` command. Plain
Enter stays a real newline. The remaining raw toolkit buttons are gone from
the production tree: one `content_button` adapter owns all native button
composition and its focus shell, while the Users session actions share the
glass button in both availability states, the Network signal-quality bar
spends the shared progress primitive, and the Applications priority preset
uses the shared select. These are
PRESENTATION executions of the shared tokens, not new semantics: the
two registered divergences above keep their reasons unchanged.

Native Iced status values from the activation and input families pass through
`ControlPaintState` and the shared `ControlInkPriority` before a style reads
theme tokens. Slider drag, opened select and focused text input therefore keep
the same precedence as GPUI buttons instead of owning local status folds.

The bounded horizontal action strip (`controls/strip.rs::bounded_toolbar`)
keeps complete groups at intrinsic width and uses the caller's declared
control role for its viewport height, without adding internal control
padding. The Applications action bar declares `ControlRole::Input` for its
priority select, while button-only navigation declares its button role. The
lower process table never loses a vertical row to a wrapped toolbar. GPUI
keeps the same vocabulary in an anchored overflow menu; this is a toolkit
presentation execution, not a new capability.

<!-- iced-capability-divergences: text-selection -->

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
