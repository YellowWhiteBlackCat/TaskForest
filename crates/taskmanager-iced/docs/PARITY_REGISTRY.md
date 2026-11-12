# Capability parity and page-level divergence

Component/surface capability parity is MACHINE-CHECKED (CORE-08):
`src/capabilities.rs` declares every ui-contract registry capability with an
explicit decision — `Ported`, `Native`, or a reasoned `Divergent`/
`Unsupported`. TextSelection is the only remaining capability-level
divergence; Scrollbar is `Native { via: "iced scrollable" }`, and the four
state-ink presentation differences are registered below:

- TextSelection: pointer/word/block selection, multi-line spans, Ctrl/Cmd-C,
  and Ctrl/Cmd-A on the readout that owns the window selection are ported; an
  unowned readout keeps Ctrl-A on the shared row-summary command because
  readouts are not keyboard-focusable.

Every other capability is `Ported`. `ComponentCapability::ALL` supplies the
shared 19-entry behavior inventory; buttons, icon buttons, pills, labels,
spinners and progress are renderer components rather than extra capability
entries. Iced's multiline Run Task editor is a local extension over the
shared single-line TextInput contract.

Tooltip follows keyboard-visible focus on any focusable descendant by
reading the real Iced focus operation state. Non-focusable footer badges and
row-detail hints remain hover-only because they have no keyboard focus target.
The keyboard path adds no second shared semantic or renderer-neutral state.

Toast is a real non-modal Card-glass card over the root view: the shared shell
`FeedbackPresentation` owns the copy, severity and lifetime, while Iced owns
the floating presentation, the shared `content_button` dismiss stop and
the local dismissal receipt. `FeedbackSeverity::status_kind()`
projects the notice's severity onto `StatusKind`; `StatusColors::resolve`
maps that kind to the status ink and `taskmanager_icons::status_icon` supplies
the semantic glyph before native rendering. This frontend holds no local
severity→ink or severity→icon table.
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
resolved through `taskmanager-application::i18n`. `Checkbox` is a real
`components::check_box` production path in the Applications column menu;
`TextInput` is mounted through `plain_text_input` on the Applications search
field, the Services filter and the process-properties environment filter.
`TextEditor` is mounted through `multi_line_text_input` on the Run Task
command field: its typed edit actions stay in the frontend's command draft,
and Ctrl/Cmd-Enter publishes the existing `SubmitRunTask` command. Plain
Enter stays a real newline. One `content_button` adapter owns all native
button composition and its focus shell; the Users session actions share the
glass button in both availability states, the Network signal-quality bar
spends the shared progress primitive, and the Applications priority preset
uses the shared select. These are PRESENTATION executions of the shared
tokens, not additional capabilities; the registered capability-level
divergence and the presentation differences below keep their reasons.

Native Iced status values from the activation and input families project into
the shared `ControlInkPriority` before a style reads theme tokens. This
frontend owns NO private status→ink table: the general fold
(`ControlInkPriority::resolve` through
`theme::ControlVisualState::from_priority`) covers `controls/state.rs`, the
toast dismiss affordance and the focus shell's row hover layer. The
selection-dominant fold
(`ControlInkPriority::resolve_selection_dominant` through
`theme::ControlVisualState::from_selection_dominant_priority`) covers the
device-rail card, the table header cell, the table row and the active
segmented segment. The menu cursor row binds the shared `Hovered` ink from
`theme::hover_ink`. Slider drag, opened select and focused text input keep
the same precedence as GPUI buttons.

A control that paints its selection as its OWN surface (the device-rail card,
the active sort column, the selected table row, the active segmented
segment) resolves the shared SELECTION-DOMINANT fold, so the durable
selection survives a pointer resting on it; every other control resolves the
general fold, where a transient hover outranks a selection. The active
segmented segment is the shared contract's own example: the GPUI segmented
control and this frontend resolve one rule at one address.

Four state-ink differences remain, all registered here rather than forked:

- Table-row hover layer: the GPUI row paints its surface as an overlay UNDER a
  transparent content tree, so a hovered row shows the shared `hover_bg`
  tint. The Iced row paints its surface INSIDE the row content, above the
  focus shell's hover layer, so the hovered tint is covered by the row's own
  fill. The ink token is the same shared `hover_bg` the GPUI row wears; only
  the layer order differs, and changing it would mean moving the row surface
  out of the row content — a table composition change, not an ink change.
- Active sort column: the GPUI sort cell paints the accent at 0.10 alpha as
  its own background and keeps a normal foreground; this frontend keeps an
  accent foreground over a transparent background. The fold decides the
  precedence in both (the active column survives a hover); only the token
  binding differs.
- Toast dismiss affordance: the GPUI dismiss changes only its text ink and
  carries no hover fill; the Iced dismiss paints a `border`-tinted fill while
  the pointer rests on it. The general shared fold supplies the verdict; the
  local fill remains an Iced presentation choice.
- Scroll affordance: the scrollbar is declared `Native { via: "iced
  scrollable" }` above — the toolkit's capsule thumb and translucent rail
  stand in for the GPUI scrollbar's hover/drag thickening. Its ink is the
  toolkit's alpha ladder, not a control-state ink, so it stays outside the
  shared fold.

The select's dropdown panel wears the shared `GlassSurfaceRole::Menu` material
and its cursor row spends the shared hover ink with the normal foreground, so
the open menu matches GPUI's dropdown menu and Iced's own context menus. The
toolkit menu paints the panel and its cursor row from one border, so the
cursor row inherits the panel's corner radius; that rounding, and the menu's
own scrollbar, stay toolkit rendering details rather than a second material
decision.

The bounded horizontal action strip (`controls/strip.rs::bounded_toolbar`)
keeps complete groups at intrinsic width and uses the caller's declared
control role for its viewport height, without adding internal control
padding. The Applications action bar declares `ControlRole::Input` for its
priority select, while button-only navigation declares its button role. The
lower process table never loses a vertical row to a wrapped toolbar. GPUI
keeps the same vocabulary in an anchored overflow menu; this is a toolkit
presentation execution, not a new capability.

The System dashboard's metric cards spend the shared card-flow
`card_height` as a FLOOR, exactly as GPUI does: one `FillPortion(1)` line per
admitted group and `Length::Fill` per card grow the card into its stretched
line, with the shared `BOTTOM_SAFETY_BAND` charged before admission and
reserved at the end of the column. Headless rendered bounds prove the last
card reaches the bottom of its line and every admitted card has an equal
height greater than its fixed chart. The trailing band stays inside the
elastic lines, so no unpainted band remains below the last row.

`CARD_CONTENT_FURNITURE_HEIGHT` is the
card's own internal title/readout/caption/padding footprint. It is NOT a
`presentation::chrome_rows::ChromeRowSpec`: that contract measures the fixed
rows a PAGE paints above its content slot (section tabs, window controls,
paging) from the shared `ControlRole` ladder, while these lines live INSIDE
one card, are text-sized rather than control-ladder-sized, and have no
`ChromeRowRole`. The classification driver is the contract's own scope, not
a convenience: binding it would invent a chrome row that the page never
paints. The literal is documented so no reader mistakes card content for
page chrome.

<!-- iced-capability-divergences: text-selection -->

Below is the complementary PAGE-LEVEL registry: Performance-page skin
divergences hanging on a shared semantic with a real Iced-vs-GPUI architecture
driver (ARCH.md §8.2). They are presentation executions of shared semantics,
not component-capability differences, so they stay prose here; anything
duplicating GPUI semantics without such a driver is removed or aligned
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
