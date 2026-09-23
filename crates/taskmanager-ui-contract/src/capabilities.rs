//! Component/surface capability coverage contract (the parity registry,
//! CORE-08).
//!
//! The keybindings matrix proves every command is explicitly bound or
//! deliberately unbound in every frontend shape; this module extends the
//! same anti-silence fold to component and surface CAPABILITIES. For every
//! capability in [`ComponentCapability::ALL`], every frontend must declare
//! one explicit [`CapabilitySupport`] decision - a silent omission is
//! drift, not a choice. Deliberate differences must carry a reason
//! ([`CapabilitySupport::Divergent`] / [`CapabilitySupport::Unsupported`]);
//! comments claiming parity do not count.
//!
//! ## The reference shape (GPUI-05)
//!
//! `taskmanager-ui` - the GPUI component layer built directly on gpui
//! (gpui 0.2.2 ships none of these components; they are hand-built there) -
//! is the SEMANTIC REFERENCE SOURCE for the parallel component layers
//! (Iced, TUI, Bevy). Each capability names its reference component through
//! [`ComponentCapability::reference_path`], and only the GPUI shape may
//! declare [`CapabilitySupport::Reference`]. A parallel frontend that
//! ports the semantics declares [`CapabilitySupport::Ported`]; a frontend
//! that must diverge declares [`CapabilitySupport::Divergent`] with the
//! driver. When porting work uncovers a gap or a bug in the reference, the
//! resolution is a write-back: fix `taskmanager-ui` (and this contract if
//! the vocabulary itself is wrong) FIRST, then port the fix - a silent
//! per-frontend fork of a reference semantic is exactly what this registry
//! exists to make impossible. Because the GPUI declaration must cover every
//! capability with `Reference`/`Native`, the vocabulary can only grow when
//! the reference layer grows.
//!
//! ## "求同存异" Governance: The Four-Frontend Parity Model
//!
//! TaskForest enforces a strict "求同存异" (seek common ground while
//! preserving differences) governance model across its four frontends:
//!
//! - Common ground ("求同"): Every frontend consumes the same typed domain
//!   facts from `core`, the same user commands ([`crate::command::descriptor`]),
//!   the same keybinding matrix ([`crate::keybindings`]), the same typed column
//!   contract ([`crate::columns::PROCESS_COLUMNS`]), and the same product
//!   intents ([`crate::functional::ProductIntent`]).
//! - Preserved differences ("存异"): Each frontend is an independent, first-class
//!   product shape with its own native toolkit paradigms. Toolkit-native
//!   divergences are intentional design assets that leverage platform strengths,
//!   never secondary compromises or incomplete ports.
//!
//! ## Accepted Difference and Divergence Quality Baselines
//!
//! When a frontend declares [`CapabilitySupport::Divergent`],
//! [`CapabilitySupport::Native`], or [`CapabilitySupport::Unsupported`], that
//! decision must be backed by a well-defined quality baseline:
//!
//! 1. **GPUI (TaskForest-G, reference shape)**:
//!    GPU-accelerated retained canvas with rem-scaled relative layout budgets
//!    ([`taskmanager_theme::UiSize`]), full floating modal/context-menu/toast
//!    stacks, live pointer-drag column resizing, and multi-pane hardware
//!    telemetry.
//! 2. **TUI (TaskForest-T, character grid and keyboard-first asset)**:
//!    Discrete character cell grid with zero dynamic memory allocations in the
//!    hot render loop (~18MB resident footprint).
//!    - *Braille Sparkline*: Uses Unicode Braille patterns (U+2800..U+28FF, 2x4 = 8-dot
//!      resolution per cell) to deliver 4x vertical and 2x horizontal resolution
//!      for historical telemetry curves in a single row.
//!    - *Keyboard-first navigation & footer activity line*: Replaces pointer hover
//!      and floating toasts with keyboard shortcuts and a dedicated non-overlapping
//!      `footer.activity-line`.
//!    - *Centered modal blocks*: Clear-scrim centered frames with absolute Escape
//!      priority.
//!    - *Honest scalar availability*: Unavailable metrics render placeholder dashes,
//!      never fabricated zeros.
//! 3. **Iced (TaskForest-I, pure functional Elm architecture asset)**:
//!    Strict The Elm Architecture (TEA) `Model -> Update -> View` with immutable
//!    state transitions and async task isolation.
//!    - *Pure functional virtual list*: `VirtualWindow` with `lazy` caching
//!      materializes only visible rows into immutable widget trees without
//!      imperative leaks.
//!    - *Single-row compact ribbons*: View presets and toolbar actions collapse into
//!      a 32px horizontal glide rail, preserving at least 7-9 rows of table data
//!      in compact 720x480 viewports.
//!    - *Footer activity line*: Operation feedback routes to the window footer,
//!      maintaining a calm, uncrowded interface.
//! 4. **Bevy UI (TaskForest-B, pure data-driven ECS asset)**:
//!    Bevy 0.19 entity-component-system graph with 100% `bsn!` declarative
//!    scene composition.
//!    - *Pure declarative scene tree*: UI nodes are reactive entities governed by
//!      components and observer systems (`commands.trigger(...)`).
//!    - *Picking transparency*: Child text and icon entities inside buttons carry
//!      `Pickable::IGNORE`, ensuring reliable pointer event dispatch to parent buttons.
//!    - *Responsive flex slot distribution*: Column layouts adapt via declarative
//!      flex slots rather than pointer-drag handles, guaranteeing scene stability.
//!    - *Dedicated status bar*: Feedback is channeled through an engine status entity.
//!
//! ## Honesty boundary
//!
//! This registry proves DECLARATION discipline - no silence, no
//! unexplained divergence, no reference-less capability. It does NOT prove
//! behavioral equivalence: each cell is backed by the shape's own behavior
//! tests and evidence route (CORE-06), and a `Ported` cell claims the
//! intent to match, never the match itself.
//!
//! ## Semantic-spec writing rule
//!
//! Each [`CapabilitySemanticSpec`] states the SEMANTIC result every shape must
//! deliver (or declare an explained divergence from). Toolkit-specific
//! presentation details - animation style, hover timing, which affordance
//! paints a control, the concrete trigger chord - belong to the reference
//! component, to the per-shape declaration, or to the shape's own charter:
//! writing them into the shared spec turns a legitimate porting difference
//! into an apparent false promise. Keep every clause checkable against
//! delivered behavior, and never claim a state or affordance no shape ships.
//!
//! ## Delivered-surface vocabulary (intentionally asymmetric)
//!
//! This registry is the DELIVERED-SURFACE vocabulary. A capability is admitted
//! only because at least one shape really ships it, and the reference shape
//! (GPUI, whose `taskmanager-ui` layer owns the semantics) must own every
//! entry. The gate therefore rejects a reference shape that declares
//! [`CapabilitySupport::Ported`], `Divergent`, or `Unsupported`
//! ([`CapabilityFindingKind::ReferenceShapeCannotDefer`]): the reference layer
//! cannot defer itself, and the vocabulary grows only when that layer grows.
//!
//! ## Semantic contract vs mounted component
//!
//! `taskmanager-ui` owns the SEMANTICS of every entry, but for some
//! capabilities it owns only a semantic CONTRACT: the capability's
//! `reference_path` names the module whose behavior defines the result, yet no
//! shape mounts that component. [`ComponentCapability::is_semantic_contract`]
//! registers those audited exceptions. The reference shape must not claim such
//! a capability through [`CapabilitySupport::Reference`], which asserts a
//! MOUNTED reference component; it declares [`CapabilitySupport::Ported`] for
//! the frontend-local composition instead. A `Reference` claim for a
//! semantic-contract capability is rejected as
//! [`CapabilityFindingKind::ReferenceComponentNotMounted`]. The registry
//! therefore distinguishes semantic CONSUMPTION from component MOUNTING
//! instead of treating the existence of a reference file as delivery proof.
//!
//! The audited semantic-contract set is exactly `SearchInput`, `Checkbox`,
//! `Tree`, and `VirtualList`: each names a `taskmanager-ui` module that owns the
//! contract, but no production path mounts that component — the reference shape
//! composes the semantics locally. Every other capability keeps
//! [`CapabilitySupport::Reference`] because an audited production path mounts
//! its `reference_path` component. This set is an AUDIT RESULT, not a proof
//! that unlisted capabilities mount their reference component: a capability
//! joins (or leaves) it only when mount evidence changes, and the set is pinned
//! by `only_the_audited_capabilities_are_semantic_contracts` in
//! `tests/headless/ui_capabilities.rs`.
//!
//! [`crate::feature_coverage`] is deliberately NOT symmetric. It is the full
//! ROADMAP coverage matrix, so its reference shape MAY declare
//! [`CapabilitySupport::Unsupported`] for a feature the reference surface has
//! not delivered yet; only the porting decisions (`Ported`/`Divergent`/
//! `Native`) are rejected there
//! ([`crate::FeatureCoverageFindingKind::ReferenceShapeCannotPort`]). Do not
//! "fix" the difference: allowing reference `Unsupported` here, or forbidding
//! it there, would erase either the delivered-surface guarantee or the honest
//! roadmap gap. The asymmetry is a contract and is pinned by a test in
//! `tests/headless/ui_feature_coverage.rs`
//! (`capability_and_feature_registries_are_deliberately_asymmetric`).

use crate::keybindings::FrontendShape;

impl FrontendShape {
    /// The shape whose component layer (`taskmanager-ui`) is the semantic
    /// reference for every capability (GPUI-05): only this shape may
    /// declare [`CapabilitySupport::Reference`].
    #[must_use]
    pub const fn is_capability_reference_shape(self) -> bool {
        matches!(self, Self::Gpui)
    }
}

/// Explicit, toolkit-neutral semantic specification for a component capability.
///
/// Rather than merely pointing to a GPUI reference source path, this contract
/// defines the required user-facing behavior, keyboard/pointer semantics, and
/// invariant expectations that every frontend implementation or accepted
/// divergence must satisfy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapabilitySemanticSpec {
    /// The capability described by this specification.
    pub capability: ComponentCapability,
    /// What the user experiences and sees when interacting with this component.
    pub user_facing_behavior: &'static str,
    /// Required keyboard navigation, focus traversal, and pointer interaction rules.
    pub keyboard_pointer_semantics: &'static str,
    /// Invariant expectations (e.g. side-effect-free cancel, bounds safety, focus containment).
    pub invariant_expectations: &'static str,
}

impl CapabilitySemanticSpec {
    /// Whether this specification describes a semantic CONTRACT rather than a
    /// mounted reference component (see
    /// [`ComponentCapability::is_semantic_contract`]).
    #[must_use]
    pub const fn is_semantic_contract(self) -> bool {
        self.capability.is_semantic_contract()
    }
}

/// One component/surface capability the product offers through its
/// frontends. The set starts from what at least one shape really ships -
/// the gate forbids silence, not absence, so capabilities join this list
/// only when a shape actually implements (or needs to refuse) them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ComponentCapability {
    /// A modal layer-stacked dialog with backdrop scrim and focus containment.
    ///
    /// - **User-facing behavior**: A focused dialog surface layered above the
    ///   application viewport with a dimming scrim, blocking pointer and keyboard
    ///   interaction with background content until confirmed or dismissed.
    /// - **Interaction semantics**: Tab/Shift+Tab cycle focus strictly within
    ///   the modal; Escape or clicking the scrim triggers cancellation; initial
    ///   focus lands on the primary or designated default control on mount.
    /// - **Invariant expectations**: Dismissal or cancellation produces zero
    ///   destructive side effects; focus cannot leak to background surfaces while
    ///   mounted; restores focus to the triggering element on close.
    ModalOverlay,

    /// A contextual action menu anchored to a targeted object.
    ///
    /// - **User-facing behavior**: A transient menu displaying operations
    ///   available for a specific row, device, or selection (e.g. End Task,
    ///   affinity, priority).
    /// - **Interaction semantics**: Triggered via pointer secondary click (right-click)
    ///   on the target; keyboard-first shapes open the same item set through their
    ///   focused-target action entry. Up/Down arrow keys traverse items;
    ///   Enter/Space activates; Escape dismisses.
    /// - **Invariant expectations**: Closes on outside click, item activation,
    ///   or window blur; actions apply strictly to the entity identity captured
    ///   at invocation; bounds are clamped within the viewport.
    ContextMenu,

    /// A dropdown action or selection menu anchored to a control affordance.
    ///
    /// - **User-facing behavior**: A transient popover menu anchored to a button
    ///   or header (e.g. table column visibility menu, filter preset picker).
    /// - **Interaction semantics**: Triggered via pointer click or Space/Enter on
    ///   the anchor; Up/Down arrow navigation; Space/Enter selects or toggles;
    ///   Escape dismisses and returns focus to the anchor.
    /// - **Invariant expectations**: Closes on outside click or focus loss;
    ///   toggles update state atomically; multi-select menus remain open across
    ///   individual item toggles.
    DropdownMenu,

    /// A lightweight transient explanation surface anchored to an element.
    ///
    /// - **User-facing behavior**: A floating badge revealing descriptive text,
    ///   keyboard shortcut hints, or status explanations on inspection.
    /// - **Interaction semantics**: Appears on pointer hover or keyboard focus;
    ///   dismisses immediately on pointer exit, pointer click, or Escape.
    /// - **Invariant expectations**: Strictly non-interactive and read-only;
    ///   never captures focus; never occludes anchor controls in a way that
    ///   obstructs interaction; terminal shapes without pointer hover route hints
    ///   to the footer line instead.
    Tooltip,

    /// A non-modal transient notification surface.
    ///
    /// - **User-facing behavior**: A floating notification banner presenting
    ///   asynchronous operation receipts, copy confirmations, or non-blocking warnings.
    /// - **Interaction semantics**: Auto-dismisses after a fixed display duration;
    ///   optional dismiss button; does not intercept or steal active typing focus.
    /// - **Invariant expectations**: Never steals input focus; dismissal is
    ///   side-effect free; terminal and compact frontends may channel feedback
    ///   through a dedicated footer activity line.
    Toast,

    /// A single-line editable text input field.
    ///
    /// - **User-facing behavior**: An interactive text entry box with visible
    ///   caret, selection highlighting, placeholder text, and text manipulation.
    /// - **Interaction semantics**: Pointer click positions caret; Left/Right arrow
    ///   moves by character (Ctrl/Alt by word); Home/End jumps to line boundaries;
    ///   Backspace/Delete removes text; Enter commits; Escape cancels or blurs.
    /// - **Invariant expectations**: Preserves buffer across view re-renders until
    ///   committed; caret respects UTF-8 character boundaries; disabled state
    ///   rejects all modifications without throwing errors.
    TextInput,

    /// A specialized type-to-filter query input.
    ///
    /// This capability is a SEMANTIC CONTRACT, not a mounted reference
    /// component ([`Self::is_semantic_contract`]): the reference layer owns the
    /// search semantics, but each shape delivers them through its own
    /// composition (GPUI's `list_view::search_box_sized`), so the reference
    /// shape declares [`CapabilitySupport::Ported`] rather than a mounted
    /// [`CapabilitySupport::Reference`].
    ///
    /// - **User-facing behavior**: A dedicated filter field equipped with a search
    ///   icon, match feedback, and instant list filtering.
    /// - **Interaction semantics**: Fast keyboard access via the shared Ctrl+F
    ///   focus-search chord; typing immediately filters active projection; Escape
    ///   clears query or restores focus to the filtered list.
    /// - **Invariant expectations**: Filtering never blocks the UI thread; an
    ///   empty query immediately restores the full projection; preserves row
    ///   selection when the selected item satisfies the new query.
    SearchInput,

    /// Read-only continuous text selection and copy surface.
    ///
    /// - **User-facing behavior**: Visual text selection highlighting across
    ///   detail readouts, scalar values, or log streams, enabling clipboard export.
    /// - **Interaction semantics**: Pointer drag selects character ranges;
    ///   double-click selects words; triple-click selects the whole readout;
    ///   Ctrl/Cmd+C copies the selection to the system clipboard.
    /// - **Invariant expectations**: At most one active text selection per window;
    ///   read-only text is never mutable; table row selection and column resize
    ///   take arbitration precedence over cell text dragging.
    TextSelection,

    /// A two-state binary toggle control.
    ///
    /// - **User-facing behavior**: An interactive switch indicating immediate
    ///   boolean state (e.g. alert rule enabled/disabled).
    /// - **Interaction semantics**: Pointer click or Space/Enter toggles state;
    ///   Tab moves focus in and out.
    /// - **Invariant expectations**: State changes dispatch typed application
    ///   intents immediately; disabled switches reject toggles and show disabled
    ///   styling; ON/OFF states remain unambiguous across all high-contrast themes.
    Switch,

    /// A bounded continuous or discrete numeric range control.
    ///
    /// - **User-facing behavior**: A slider track with filled range and draggable
    ///   thumb representing numeric settings (e.g. refresh interval, threshold).
    /// - **Interaction semantics**: Pointer drag on thumb or click on track;
    ///   Left/Down decreases by step; Right/Up increases by step; Home/End jumps
    ///   to min/max boundaries.
    /// - **Invariant expectations**: Values are strictly clamped within [min, max];
    ///   step granularity is enforced; terminal frontends without pointer analog
    ///   axis project this as discrete selectable option lists.
    Slider,

    /// A two-state boolean selection control.
    ///
    /// This capability is a SEMANTIC CONTRACT, not a mounted reference
    /// component ([`Self::is_semantic_contract`]): `inputs/checkbox.rs` defines
    /// the toggle contract, but no shape mounts it — the reference shape
    /// composes the same two-state selection through checked menu entries
    /// (`processes_view::chrome::columns`) and toggle pills — so the reference
    /// shape declares [`CapabilitySupport::Ported`].
    ///
    /// - **User-facing behavior**: A labeled checkbox reflecting checked or
    ///   unchecked state (e.g. column visibility, alert-rule toggles).
    /// - **Interaction semantics**: Pointer click on box or label, or keyboard
    ///   Space key, toggles selection state; Tab traverses focus.
    /// - **Invariant expectations**: Clicking the text label activates the box;
    ///   checked and unchecked states stay visually distinct; keyboard and
    ///   pointer triggers yield identical transitions.
    Checkbox,

    /// A single-choice selection control over an enumerated set.
    ///
    /// - **User-facing behavior**: A compact button displaying the current choice
    ///   which expands into an option list upon activation (e.g. skin, theme mode).
    /// - **Interaction semantics**: Pointer click or Space/Enter opens option menu;
    ///   Up/Down arrows navigate options; Enter commits choice; Escape cancels;
    ///   Tab moves to next control.
    /// - **Invariant expectations**: The currently selected option is always
    ///   prominently marked; selecting the active option is an idempotent no-op;
    ///   popover lists scroll cleanly when exceeding viewport bounds.
    Select,

    /// A compact exclusive choice group.
    ///
    /// - **User-facing behavior**: A horizontal ribbon of mutually exclusive tabs
    ///   or pill buttons (e.g. page navigation tabs, device category selector).
    /// - **Interaction semantics**: Pointer click activates segment; Left/Right
    ///   arrows navigate between segments; number shortcuts (1..7) jump directly
    ///   to corresponding tabs.
    /// - **Invariant expectations**: Exactly one segment is active at any time;
    ///   active segment has high-contrast visual distinction; switching tabs
    ///   preserves background data models without corruption.
    SegmentedControl,

    /// A multi-column tabular data grid with header, sorting, and row selection.
    ///
    /// - **User-facing behavior**: Structured data table presenting rows and typed
    ///   columns with sticky header, sort direction arrows, and selection highlights.
    /// - **Interaction semantics**: Up/Down arrows traverse rows; Home/End jumps to
    ///   extremes; PageUp/PageDown moves by page; pointer click selects row;
    ///   header click toggles column sort (asc/desc/none); Enter/double-click
    ///   triggers default row action.
    /// - **Invariant expectations**: Columns adhere strictly to the shared
    ///   `PROCESS_COLUMNS` contract; row identity is stable (`ProcessRowId`);
    ///   sorting and filtering never alter underlying process identities;
    ///   sticky header stays pinned during vertical scroll.
    Table,

    /// Pointer-driven interactive column width resizing.
    ///
    /// - **User-facing behavior**: Draggable divider handles at the trailing edge
    ///   of table column header cells allowing dynamic width adjustments.
    /// - **Interaction semantics**: Pointer hover over trailing border shows
    ///   resize cursor (<->); mousedown starts live drag session; mouseup commits
    ///   final width; double-click auto-fits to content.
    /// - **Invariant expectations**: Column width is bounded within `[min_width, max_width]`;
    ///   adjacent columns adapt gracefully; frontends without pointer dragging
    ///   (such as TUI) manage widths via fixed profiles or auto-fit allocations.
    ColumnDragResize,

    /// Bounded-window virtualized list rendering over large collections.
    ///
    /// This capability is a SEMANTIC CONTRACT, not a mounted reference
    /// component ([`Self::is_semantic_contract`]): `data/virtual_list.rs` owns
    /// the variable-size window algorithm, the deferred scroll handle, and the
    /// visible-range scan, but no shape mounts its `VirtualList` element — the
    /// mounted reference table renders gpui's `uniform_list` and consumes only
    /// the module's range helpers — so the reference shape declares
    /// [`CapabilitySupport::Ported`].
    ///
    /// - **User-facing behavior**: Smoothly scrolling list capable of handling
    ///   thousands of rows without latency, dropped frames, or visual tearing.
    /// - **Interaction semantics**: Responds smoothly to mouse wheel, trackpad scroll,
    ///   scrollbar thumb dragging, and keyboard navigation (Up/Down, PageUp/PageDown).
    /// - **Invariant expectations**: Only visible rows plus a bounded overscan
    ///   buffer are materialized into layout nodes (O(visible) resource bound);
    ///   selection and focus states remain consistent across virtualization bounds.
    VirtualList,

    /// A hierarchical expandable and collapsible tree structure.
    ///
    /// This capability is a SEMANTIC CONTRACT, not a mounted reference
    /// component ([`Self::is_semantic_contract`]): `data/tree.rs` owns the
    /// hierarchy contract, but no shape mounts it — the reference shape
    /// delivers the same expand/collapse semantics through its process-tree row
    /// projection (`processes_view::rows::projection`) — so the reference shape
    /// declares [`CapabilitySupport::Ported`].
    ///
    /// - **User-facing behavior**: Nested tree nodes with disclosure chevrons,
    ///   visual indentation guides, and parent-child aggregation (e.g. application
    ///   process groups).
    /// - **Interaction semantics**: Pointer click on disclosure chevron or Space
    ///   key toggles node expansion; Left arrow collapses open node or jumps to
    ///   parent; Right arrow expands closed node or jumps to first child.
    /// - **Invariant expectations**: Expanding or collapsing preserves child
    ///   selection states; summary parent rows aggregate child metrics truthfully
    ///   without fabricating zero values for unavailable child metrics.
    Tree,

    /// A visual scroll affordance bound to a tracked viewport.
    ///
    /// - **User-facing behavior**: A draggable scrollbar thumb running along a
    ///   track indicating current scroll position and visible ratio.
    /// - **Interaction semantics**: Pointer drag on thumb scrolls content proportionally;
    ///   track click jumps by page; hover highlights thumb; fades when inactive
    ///   per theme motion settings.
    /// - **Invariant expectations**: Thumb height reflects `viewport_extent / content_extent`
    ///   clamped to a visible minimum; never occludes interactive content along
    ///   the viewport margin; key-driven terminal frontends may omit pointer rail.
    Scrollbar,

    /// Modality-aware high-contrast keyboard focus indication.
    ///
    /// - **User-facing behavior**: A distinct, high-contrast focus ring surrounding
    ///   the active control when navigating via keyboard.
    /// - **Interaction semantics**: Becomes visible immediately upon keyboard navigation
    ///   (Tab, Shift+Tab, arrow keys); suppressed on mouse clicks until next keyboard
    ///   interaction.
    /// - **Invariant expectations**: Focus ring meets accessibility contrast requirements;
    ///   is never clipped by parent container overflow; clears cleanly when window
    ///   loses focus or target unmounts.
    FocusVisible,
}

impl ComponentCapability {
    /// Every capability in canonical order; declarations fold against this
    /// set the way binding declarations fold against the shared command
    /// set (`CommandId::ALL`).
    pub const ALL: &'static [Self] = &[
        Self::ModalOverlay,
        Self::ContextMenu,
        Self::DropdownMenu,
        Self::Tooltip,
        Self::Toast,
        Self::TextInput,
        Self::SearchInput,
        Self::TextSelection,
        Self::Switch,
        Self::Slider,
        Self::Checkbox,
        Self::Select,
        Self::SegmentedControl,
        Self::Table,
        Self::ColumnDragResize,
        Self::VirtualList,
        Self::Tree,
        Self::Scrollbar,
        Self::FocusVisible,
    ];

    /// Stable machine name for gates and diagnostics.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::ModalOverlay => "modal-overlay",
            Self::ContextMenu => "context-menu",
            Self::DropdownMenu => "dropdown-menu",
            Self::Tooltip => "tooltip",
            Self::Toast => "toast",
            Self::TextInput => "text-input",
            Self::SearchInput => "search-input",
            Self::TextSelection => "text-selection",
            Self::Switch => "switch",
            Self::Slider => "slider",
            Self::Checkbox => "checkbox",
            Self::Select => "select",
            Self::SegmentedControl => "segmented-control",
            Self::Table => "table",
            Self::ColumnDragResize => "column-drag-resize",
            Self::VirtualList => "virtual-list",
            Self::Tree => "tree",
            Self::Scrollbar => "scrollbar",
            Self::FocusVisible => "focus-visible",
        }
    }

    /// The `taskmanager-ui` source path (relative to `crates/taskmanager-ui/
    /// src/`) of the component that owns this capability's reference
    /// semantics (GPUI-05). A root-level existence gate keeps the pairing
    /// real: renaming or removing the reference component breaks the gate
    /// instead of silently orphaning the parallel frontends' declarations.
    /// For a capability registered as [`Self::is_semantic_contract`] this names
    /// the module that owns the semantics, not a component any shape mounts.
    #[must_use]
    pub const fn reference_path(self) -> &'static str {
        match self {
            Self::ModalOverlay => "overlays/dialog.rs",
            Self::ContextMenu => "overlays/context_menu.rs",
            Self::DropdownMenu => "overlays/dropdown_menu.rs",
            Self::Tooltip => "primitives/tooltip.rs",
            Self::Toast => "overlays/toast.rs",
            Self::TextInput => "inputs/text_input.rs",
            Self::SearchInput => "inputs/search_input.rs",
            Self::TextSelection => "primitives/selectable_text.rs",
            Self::Switch => "inputs/switch.rs",
            Self::Slider => "inputs/slider.rs",
            Self::Checkbox => "inputs/checkbox.rs",
            Self::Select => "inputs/select.rs",
            Self::SegmentedControl => "primitives/segmented.rs",
            Self::Table => "data/table.rs",
            Self::ColumnDragResize => "data/table/resize.rs",
            Self::VirtualList => "data/virtual_list.rs",
            Self::Tree => "data/tree.rs",
            Self::Scrollbar => "primitives/scrollbar.rs",
            // Focus-visible is ring composition, not the modal focus policy.
            // `palette.ring` (owned by `taskmanager-theme`) already encodes the
            // keyboard/pointer modality decision in its alpha; each interactive
            // component composes it inline in its `.focus` hook over the
            // `theme_binding::hsla` conversion. `primitives/button.rs` is the
            // canonical control and idiom. The modality signal itself is
            // frontend-owned (GPUI: `gpui_app/root/input_modality.rs`); the
            // app-layer ring adapter is `gpui_app/elements/visual.rs`
            // (`focus_ring`). `focus.rs` owns the modal trap/restore chain
            // instead — that is ModalOverlay's invariant, not this capability's.
            Self::FocusVisible => "primitives/button.rs",
        }
    }

    /// Whether the reference layer owns only this capability's SEMANTICS
    /// rather than a mounted component. The audited set is `SearchInput`,
    /// `Checkbox`, `Tree`, and `VirtualList`: for each, a `taskmanager-ui`
    /// module defines the contract, yet no production path mounts that
    /// component, so the reference shape reaches the same result through
    /// frontend-local composition and cannot claim a mounted reference
    /// component ([`CapabilitySupport::Reference`]). The set is an audit
    /// result; every other capability keeps `Reference` because an audited
    /// production path mounts its `reference_path` component. The rule and its
    /// finding are pinned by
    /// `only_the_audited_capabilities_are_semantic_contracts` and
    /// `semantic_contract_capability_cannot_claim_a_mounted_reference_component`
    /// in `tests/headless/ui_capabilities.rs`.
    #[must_use]
    pub const fn is_semantic_contract(self) -> bool {
        matches!(
            self,
            Self::SearchInput | Self::Checkbox | Self::VirtualList | Self::Tree
        )
    }

    /// Returns the explicit, toolkit-neutral semantic specification for this
    /// capability, defining the required user-facing behavior, interaction
    /// semantics, and invariant expectations.
    #[must_use]
    pub const fn semantic_spec(self) -> CapabilitySemanticSpec {
        match self {
            Self::ModalOverlay => CapabilitySemanticSpec {
                capability: self,
                user_facing_behavior: "Modal layer-stacked dialog surface with backdrop scrim blocking interaction with underlying views",
                keyboard_pointer_semantics: "Tab and Shift+Tab cycle focus strictly within modal bounds; Escape or scrim click requests dismissal; autofocuses primary or initial control on mount",
                invariant_expectations: "Dismissal, cancellation, or Escape produces zero destructive side effects; focus cannot escape to underlying layers while mounted; restores focus to trigger on close",
            },
            Self::ContextMenu => CapabilitySemanticSpec {
                capability: self,
                user_facing_behavior: "Transient contextual menu displaying operations available for a targeted object or selection",
                keyboard_pointer_semantics: "Triggered via pointer secondary click on the targeted row or element; keyboard-first shapes open the same item set through their focused-target action entry; Up/Down arrow keys traverse items; Enter/Space activates item; Escape closes",
                invariant_expectations: "Closes on outside click, item activation, or window blur; actions apply strictly to snapshot identity captured at menu invocation; clamped within viewport bounds",
            },
            Self::DropdownMenu => CapabilitySemanticSpec {
                capability: self,
                user_facing_behavior: "Transient popover menu anchored to a specific control affordance for option selection or column visibility toggles",
                keyboard_pointer_semantics: "Triggered via pointer click or Space/Enter on anchor; Up/Down arrow navigation; Space/Enter selects or toggles item; Escape closes and restores focus to anchor",
                invariant_expectations: "Closes on outside click or focus loss; updates state atomically; does not close on item toggle if multi-selection is enabled",
            },
            Self::Tooltip => CapabilitySemanticSpec {
                capability: self,
                user_facing_behavior: "Lightweight transient explanation bubble revealing contextual label, shortcut hint, or status detail on inspection",
                keyboard_pointer_semantics: "Appears on pointer hover or on keyboard focus; dismisses immediately on pointer exit, pointer click, or Escape",
                invariant_expectations: "Strictly non-interactive and read-only; never captures or traps keyboard/pointer focus; never blocks interaction with underlying anchor control; terminal shapes route hints to footer line",
            },
            Self::Toast => CapabilitySemanticSpec {
                capability: self,
                user_facing_behavior: "Non-modal transient notification banner displaying operation feedback, copy receipts, or non-blocking warnings",
                keyboard_pointer_semantics: "Auto-dismisses after fixed timeout; optional dismiss button accessible via keyboard or click; does not intercept active input focus",
                invariant_expectations: "Never steals input focus; dismissal is side-effect free; terminal and compact frontends may channel feedback through a dedicated footer activity line",
            },
            Self::TextInput => CapabilitySemanticSpec {
                capability: self,
                user_facing_behavior: "Single-line editable text input field with visible caret, selection highlighting, and text manipulation",
                keyboard_pointer_semantics: "Pointer click positions caret; Left/Right moves caret; Home/End jumps to line boundaries; Backspace/Delete removes characters; Enter commits; Escape cancels or unfocuses",
                invariant_expectations: "Preserves buffer until committed or canceled; caret respects UTF-8 character boundaries; disabled state rejects modifications without mutation",
            },
            Self::SearchInput => CapabilitySemanticSpec {
                capability: self,
                user_facing_behavior: "Specialized type-to-filter query input with search icon, Escape-clears contract, match feedback, and instant list filtering",
                keyboard_pointer_semantics: "Fast keyboard access via the shared Ctrl+F focus-search chord; typing immediately filters active projection; Escape clears query or restores focus to filtered list",
                invariant_expectations: "Filtering never blocks UI thread; empty query restores full projection; preserves active row selection when matched item remains in view",
            },
            Self::TextSelection => CapabilitySemanticSpec {
                capability: self,
                user_facing_behavior: "Continuous text selection highlighting across read-only data, detail labels, or log streams enabling clipboard export",
                keyboard_pointer_semantics: "Pointer drag selects character ranges; double-click selects words; triple-click selects the whole readout; Ctrl/Cmd+C copies the selection to the system clipboard",
                invariant_expectations: "At most one active text selection per window; read-only text is never mutable; table row selection and column resize take arbitration precedence",
            },
            Self::Switch => CapabilitySemanticSpec {
                capability: self,
                user_facing_behavior: "Two-state binary toggle control indicating immediate boolean state",
                keyboard_pointer_semantics: "Pointer click or Space/Enter toggles boolean state; Tab navigates focus",
                invariant_expectations: "State changes dispatch typed application intents immediately; disabled state prevents toggle; visually distinct ON and OFF presentation across all themes",
            },
            Self::Slider => CapabilitySemanticSpec {
                capability: self,
                user_facing_behavior: "Bounded continuous or discrete step numeric range control with track, fill, and draggable thumb",
                keyboard_pointer_semantics: "Pointer drag on thumb or click on track; Left/Down decreases by step; Right/Up increases by step; Home/End jumps to min/max bounds",
                invariant_expectations: "Values clamped strictly within [min, max]; step granularity enforced; terminal frontends without pointer analog axis project as discrete selectable option lists",
            },
            Self::Checkbox => CapabilitySemanticSpec {
                capability: self,
                user_facing_behavior: "Two-state selection control with box indicator and label",
                keyboard_pointer_semantics: "Pointer click on box or label, or keyboard Space key, toggles selection state; Tab traverses focus",
                invariant_expectations: "Clicking label toggles checkbox; checked and unchecked states are visually distinct; identical state transitions across pointer and keyboard",
            },
            Self::Select => CapabilitySemanticSpec {
                capability: self,
                user_facing_behavior: "Single-choice picker over an enumerated set of options",
                keyboard_pointer_semantics: "Pointer click or Space/Enter opens popover list; Up/Down arrow keys highlight options; Enter commits selection; Escape cancels; Tab moves focus",
                invariant_expectations: "Current selection clearly displayed; selecting active choice is a no-op; popover list is bounded and scrollable when option count exceeds viewport",
            },
            Self::SegmentedControl => CapabilitySemanticSpec {
                capability: self,
                user_facing_behavior: "Compact horizontal ribbon of mutually exclusive tabs or pill buttons",
                keyboard_pointer_semantics: "Pointer click on segment; Left/Right arrow keys traverse segments; direct digit shortcuts (1..7) activate corresponding segment",
                invariant_expectations: "Exactly one segment active at any time; active segment has high-contrast visual distinction; switching tabs preserves background data models without corruption",
            },
            Self::Table => CapabilitySemanticSpec {
                capability: self,
                user_facing_behavior: "Multi-column tabular data grid with sticky header, typed column cells, sorting indicator, and row selection",
                keyboard_pointer_semantics: "Up/Down arrows navigate rows; Home/End jumps to extremes; PageUp/PageDown scrolls pages; pointer click selects row; header click sorts; Enter/double-click triggers default row action",
                invariant_expectations: "Columns adhere strictly to shared PROCESS_COLUMNS contract; row identity is stable; sorting and filtering never alter underlying process identities; sticky header pins during scroll",
            },
            Self::ColumnDragResize => CapabilitySemanticSpec {
                capability: self,
                user_facing_behavior: "Draggable divider handles at trailing edge of table column header cells allowing dynamic width adjustments",
                keyboard_pointer_semantics: "Pointer hover over trailing border shows horizontal resize cursor; mousedown starts live drag session; mouseup commits final width; double-click auto-fits content",
                invariant_expectations: "Column width clamped to [min_width, max_width]; adjacent columns adapt or shift gracefully; frontends without pointer dragging manage widths via fixed profiles or auto-fit allocations",
            },
            Self::VirtualList => CapabilitySemanticSpec {
                capability: self,
                user_facing_behavior: "High-performance virtualized list rendering over large collections with smooth scrolling",
                keyboard_pointer_semantics: "Responds smoothly to pointer wheel, trackpad scroll, scrollbar thumb dragging, and keyboard navigation (Up/Down, PageUp/PageDown, Home/End)",
                invariant_expectations: "Materializes only visible rows plus bounded overscan buffer (O(visible) resource bound); zero frame stutter; selection and focus remain valid across virtualization bounds",
            },
            Self::Tree => CapabilitySemanticSpec {
                capability: self,
                user_facing_behavior: "Hierarchical expandable and collapsible tree structure with nested nodes, disclosure chevrons, and parent-child aggregation",
                keyboard_pointer_semantics: "Pointer click on disclosure chevron or Space toggles expand/collapse; Left arrow collapses node or jumps to parent; Right arrow expands or jumps to first child",
                invariant_expectations: "Expanding or collapsing preserves child selection states; summary parent rows aggregate child metrics truthfully without fabricating zero values for unavailable child metrics",
            },
            Self::Scrollbar => CapabilitySemanticSpec {
                capability: self,
                user_facing_behavior: "Visual scroll affordance indicating viewport position and extent within scrollable region, with draggable thumb and track",
                keyboard_pointer_semantics: "Pointer drag on thumb scrolls content proportionally; track click jumps by page; hover highlights thumb; fades when inactive per theme motion settings",
                invariant_expectations: "Thumb size accurately reflects visible ratio (viewport_extent / content_extent) clamped to readable minimum; never occludes interactive content; key-driven terminal frontends may omit pointer rail",
            },
            Self::FocusVisible => CapabilitySemanticSpec {
                capability: self,
                user_facing_behavior: "Modality-aware high-contrast visual focus indicator around active interactive control during keyboard navigation",
                keyboard_pointer_semantics: "Appears immediately on keyboard traversal (Tab, Shift+Tab, arrow keys); suppressed during mouse/pointer clicks until next keyboard interaction",
                invariant_expectations: "Focus ring meets accessibility contrast requirements; never clipped by parent overflow; cleared cleanly when window loses focus or target unmounts",
            },
        }
    }
}

/// One frontend's support decision for a capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapabilitySupport {
    /// The reference semantics themselves, owned by `taskmanager-ui`. For a
    /// component capability this asserts a MOUNTED reference component: the
    /// shape renders the component named by
    /// [`ComponentCapability::reference_path`]. Only the GPUI shape may declare
    /// this (GPUI-05), and never for a capability registered as
    /// [`ComponentCapability::is_semantic_contract`] (see
    /// [`CapabilityFindingKind::ReferenceComponentNotMounted`]).
    Reference,
    /// The toolkit/platform supplies the mechanism and its semantics;
    /// `via` names the supplier (e.g. "iced scrollable", "terminal
    /// emulator selection").
    Native { via: &'static str },
    /// A frontend-local port of the reference semantics, with the intent to
    /// match them (behavior proof stays with the shape's tests). For a
    /// semantic-contract capability this is also the reference shape's honest
    /// declaration: it composes the reference semantics locally instead of
    /// mounting a reference component.
    Ported,
    /// Deliberate divergence from the reference semantics, with the
    /// architecture driver stated.
    Divergent { reason: &'static str },
    /// Typed not-offered in this shape, with the driver stated.
    Unsupported { reason: &'static str },
}

impl CapabilitySupport {
    /// The explanatory text this decision must carry — `None` for
    /// `Reference`/`Ported`, the supplier or reason otherwise.
    #[must_use]
    pub const fn explanation(self) -> Option<&'static str> {
        match self {
            Self::Reference | Self::Ported => None,
            Self::Native { via } => Some(via),
            Self::Divergent { reason } | Self::Unsupported { reason } => Some(reason),
        }
    }
}

/// One declared capability-to-support pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapabilityEntry {
    pub capability: ComponentCapability,
    pub support: CapabilitySupport,
}

/// A frontend's explicit declaration of its component/surface capability
/// surface: one entry per contract-known capability.
#[derive(Clone, Debug)]
pub struct FrontendCapabilityDeclaration {
    pub frontend: FrontendShape,
    pub entries: Vec<CapabilityEntry>,
}

/// The DECLARATION-DRIFT outcome for one capability.
///
/// This is the frontend declaration axis, not the platform availability axis:
/// it reports whether a frontend declared a capability explicitly
/// (`Declared`), or silently omitted/duplicated/over-declared it. It is
/// deliberately NOT named `CapabilityStatus`, which is owned by
/// `taskmanager-platform-contract` and describes runtime availability. The
/// self-describing name also keeps it distinct from the sibling coverage axes
/// ([`crate::BindingCoverageStatus`] for command bindings and
/// [`crate::FeatureCoverageStatus`] for product features).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapabilityCoverageStatus {
    /// An explicit support decision.
    Declared(CapabilitySupport),
    /// Contract-known but absent from the declaration — a silent omission.
    Missing,
    /// Declared more than once by the same frontend.
    Duplicated,
    /// Declared but outside the known capability set.
    Unknown,
}

impl CapabilityCoverageStatus {
    /// Whether the capability carries an explicit decision — the only
    /// status a no-drift declaration may show.
    #[must_use]
    pub const fn is_explicit(self) -> bool {
        matches!(self, Self::Declared(_))
    }

    /// Whether this status is a drift finding a gate must reject.
    #[must_use]
    pub const fn is_drift(self) -> bool {
        !self.is_explicit()
    }
}

/// The coverage matrix for one declaration against the full capability set,
/// in canonical [`ComponentCapability::ALL`] order.
#[must_use]
pub fn capability_report(
    declaration: &FrontendCapabilityDeclaration,
) -> Vec<(ComponentCapability, CapabilityCoverageStatus)> {
    capability_report_over(declaration, ComponentCapability::ALL)
}

/// The drift findings alone.
#[must_use]
pub fn capability_drift(
    report: &[(ComponentCapability, CapabilityCoverageStatus)],
) -> Vec<(ComponentCapability, CapabilityCoverageStatus)> {
    report
        .iter()
        .copied()
        .filter(|(_, status)| status.is_drift())
        .collect()
}

/// What a capability gate rejects, beyond the plain drift statuses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapabilityFindingKind {
    /// Contract-known capability absent from the declaration.
    Missing,
    /// Capability declared more than once.
    Duplicated,
    /// Declared capability outside the known set.
    Unknown,
    /// A non-reference shape declared `Reference` (GPUI-05: only the GPUI
    /// shape owns reference semantics).
    ReferenceOutsideReferenceShape,
    /// The reference shape declared `Ported`/`Divergent`/`Unsupported` —
    /// the shape that owns the semantics cannot port, diverge from, or
    /// defer itself; grow (or shrink) the shared vocabulary instead. The one
    /// exception is `Ported` for a capability registered as
    /// [`ComponentCapability::is_semantic_contract`]: the reference layer owns
    /// only that capability's semantics, so the shape's frontend-local
    /// composition is the only honest delivery claim.
    ReferenceShapeCannotDefer,
    /// The reference shape declared [`CapabilitySupport::Reference`] for a
    /// capability registered as [`ComponentCapability::is_semantic_contract`]:
    /// the reference layer owns only that capability's semantics, no shape
    /// mounts the component at `reference_path`, and the honest declaration is
    /// [`CapabilitySupport::Ported`]. This keeps "the reference file exists"
    /// from being read as "the reference shape mounts a control".
    ReferenceComponentNotMounted,
    /// A `Native`/`Divergent`/`Unsupported` decision without its required
    /// supplier/reason text.
    EmptyExplanation,
}

/// One gate finding: the frontend, the capability, and what is wrong.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapabilityFinding {
    pub frontend: FrontendShape,
    pub capability: ComponentCapability,
    pub kind: CapabilityFindingKind,
}

/// The one-call gate payload: fold drift plus the shape-discipline rules
/// (reference role, reference-shape totality, non-empty explanations). An
/// empty result is the only passing state.
#[must_use]
pub fn capability_findings(declaration: &FrontendCapabilityDeclaration) -> Vec<CapabilityFinding> {
    let drift_kind = |status: CapabilityCoverageStatus| match status {
        CapabilityCoverageStatus::Missing => Some(CapabilityFindingKind::Missing),
        CapabilityCoverageStatus::Duplicated => Some(CapabilityFindingKind::Duplicated),
        CapabilityCoverageStatus::Unknown => Some(CapabilityFindingKind::Unknown),
        CapabilityCoverageStatus::Declared(_) => None,
    };
    let mut findings: Vec<CapabilityFinding> = capability_report(declaration)
        .into_iter()
        .filter_map(|(capability, status)| {
            drift_kind(status).map(|kind| CapabilityFinding {
                frontend: declaration.frontend,
                capability,
                kind,
            })
        })
        .collect();
    for (capability, status) in capability_report(declaration) {
        let CapabilityCoverageStatus::Declared(support) = status else {
            continue;
        };
        if let Some(kind) = support_finding_kind(declaration.frontend, capability, support) {
            findings.push(CapabilityFinding {
                frontend: declaration.frontend,
                capability,
                kind,
            });
        }
    }
    findings.sort_by_key(|finding| finding.capability);
    findings.dedup();
    findings
}

/// The shape-discipline finding for one explicit declaration, independent of
/// drift. Kept separate so [`capability_findings`] stays a flat fold and the
/// semantic-contract exception stays in one place.
fn support_finding_kind(
    frontend: FrontendShape,
    capability: ComponentCapability,
    support: CapabilitySupport,
) -> Option<CapabilityFindingKind> {
    let reference_shape = frontend.is_capability_reference_shape();
    let overclaim = if support == CapabilitySupport::Reference {
        if !reference_shape {
            Some(CapabilityFindingKind::ReferenceOutsideReferenceShape)
        } else if capability.is_semantic_contract() {
            Some(CapabilityFindingKind::ReferenceComponentNotMounted)
        } else {
            None
        }
    } else if reference_shape
        && matches!(
            support,
            CapabilitySupport::Ported
                | CapabilitySupport::Divergent { .. }
                | CapabilitySupport::Unsupported { .. }
        )
        && !(support == CapabilitySupport::Ported && capability.is_semantic_contract())
    {
        Some(CapabilityFindingKind::ReferenceShapeCannotDefer)
    } else {
        None
    };
    overclaim.or_else(|| {
        support
            .explanation()
            .is_some_and(str::is_empty)
            .then_some(CapabilityFindingKind::EmptyExplanation)
    })
}

/// The fold against a restricted known set — the seam that keeps the
/// `Unknown` path real (a declared capability outside `known` is reported,
/// never silently dropped).
fn capability_report_over(
    declaration: &FrontendCapabilityDeclaration,
    known: &[ComponentCapability],
) -> Vec<(ComponentCapability, CapabilityCoverageStatus)> {
    let mut report: Vec<(ComponentCapability, CapabilityCoverageStatus)> = known
        .iter()
        .map(|capability| (*capability, CapabilityCoverageStatus::Missing))
        .collect();
    for entry in &declaration.entries {
        match report
            .iter_mut()
            .find(|(capability, _)| *capability == entry.capability)
        {
            Some((_, status)) => {
                if status.is_explicit() || matches!(status, CapabilityCoverageStatus::Duplicated) {
                    *status = CapabilityCoverageStatus::Duplicated;
                } else {
                    *status = CapabilityCoverageStatus::Declared(entry.support);
                }
            }
            None => report.push((entry.capability, CapabilityCoverageStatus::Unknown)),
        }
    }
    report
}

#[cfg(test)]
#[path = "../tests/headless/ui_capabilities.rs"]
mod tests;
