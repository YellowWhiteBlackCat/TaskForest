//! Shared scaffolding for the Services + Startup list views.
//!
//! Both views are structured identically — a status-filter pill row + a search
//! box over a virtualized Table, with an action bar that surfaces transient
//! `ActionFeedback` — so this module holds the pieces that are truly identical
//! between them, letting [`crate::gpui_app::services_view`] and
//! [`crate::gpui_app::startup_view`] drop ~70% of their duplicated boilerplate.
//! Anything that differs materially between the two pages (the `TableDelegate`
//! impls, the per-page action-button set, the lazy Service-Details deps fetch)
//! stays in-place in its own view — this is a conservative extraction, not a
//! full generic list framework.
//!
//! What lives here:
//!   * [`ActionFeedback`] — the transient action-outcome payload (struct +
//!     [`ActionFeedback::from_result`] constructor + accessors), previously
//!     duplicated as private structs in both views.
//!   * [`FilterSpec`] — the `Copy` contract a status-filter enum (services'
//!     `ServiceFilter`, startup's `StartupFilter`) implements so the generic
//!     [`filter_pill_row`] / `filter_pill` helpers can render it.
//!   * [`filter_pill_row`] / `filter_pill` — the status-filter pill cluster,
//!     generic over `F: FilterSpec`.
//!   * [`search_box`] — the recurring `gpui_component::input::Input` bound to a
//!     persistent `Entity<InputState>`.
//!   * [`empty_state`] — the centered "No \<noun\>" / `No \<noun\> match "\<q\>"`
//!     hint shown when a filtered list has zero rows.
//!   * [`feedback_status_line`] — the action bar's status line (feedback text
//!     when present, else a caller-supplied selection hint).

use crate::gpui_app::elements;
use crate::gpui_app::root::{Hover, RootView};
use gpui::{
    AnyElement, App, Div, Entity, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, Window, div, px,
};
use taskmanager_application::i18n;
use taskmanager_application::{RefreshRequest, SourceStateKind, merge_source_lines};
use taskmanager_core::core::FailureKind;
use taskmanager_core::core::source::SourceStatus;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;

use taskmanager_theme::Palette;
use taskmanager_ui::inputs::text_input::{TextInput, TextInputState};
use taskmanager_ui::primitives::button::{Button, ButtonState, ButtonVariant};
use taskmanager_ui::primitives::pill::{Pill, PillState};
use taskmanager_ui::primitives::state_panel::StatePanel;
use taskmanager_ui_contract::IconId;

/// Debug-selector identity of the shared list-page inner shell.
///
/// Shared between the builder and the render-path guard (ADR-042) so the
/// assertion can never spell a drifted selector.
pub const LIST_PAGE_SCAFFOLD_SELECTOR: &str = "tm-list-page-scaffold";

/// Shared page shell for list-oriented top-level views.
///
/// The header is intentionally an already-composed element: Services,
/// Startup, and Users have different controls, but all three need the same
/// shrinkable body boundary below those controls. Keeping that boundary here
/// prevents each page from inventing a slightly different flex contract.
pub struct ListPageScaffold {
    header: AnyElement,
    body: AnyElement,
}

impl ListPageScaffold {
    /// Build a list-page shell from its page-specific header and body.
    pub fn new(header: impl IntoElement, body: impl IntoElement) -> Self {
        Self {
            header: header.into_any_element(),
            body: body.into_any_element(),
        }
    }

    /// Render the common full-size column with a bounded, shrinkable body.
    #[must_use]
    pub fn render(self) -> Div {
        div()
            .flex()
            .flex_col()
            .gap(taskmanager_ui::theme_binding::definite_length(
                tokens::SPACE_8,
            ))
            .size_full()
            // The list-page family's ONE inner shell (ADR-042): header band
            // + bounded body, shared by every inventory page so their
            // chrome/list separation adjusts in one place.
            .debug_selector(|| LIST_PAGE_SCAFFOLD_SELECTOR.to_string())
            .child(self.header)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w(px(0.0))
                    .min_h(px(0.0))
                    .child(self.body),
            )
    }
}

// ── ActionFeedback (transient action-outcome payload) ─────────────────────────

/// Outcome of a single list-page action (Start / Stop / Restart / Enable /
/// Disable / ...), surfaced transiently in the action bar's status line so
/// actions don't swallow `Result`s silently (U3-3). Held on `RootView` as
/// `Option<ActionFeedback>` (one slot per page: `services_state.feedback` /
/// `startup_state.feedback`); each action overwrites it (lean v1, no tick timer
/// — endorsed by U3-3).
///
/// `is_error` picks the status-line color; `text` is the rendered (single-line)
/// message. Fields are private: callers construct via
/// [`ActionFeedback::from_result`] and read via [`ActionFeedback::is_error`] /
/// [`ActionFeedback::text`].
///
/// Shared between services + startup + users (each re-exports it as
/// `services_view::ActionFeedback` / `startup_view::ActionFeedback` /
/// `users_view::ActionFeedback` so the existing `RootView` state paths keep
/// resolving without touching `root.rs`).
#[derive(Clone, Debug)]
pub struct ActionFeedback {
    is_error: bool,
    text: String,
}

impl ActionFeedback {
    /// Build a feedback value from an action's outcome. Called from the
    /// action-bar button closures after the underlying manager call returns;
    /// the caller assigns the result into `RootView.<page>_state.feedback`, and
    /// [`feedback_status_line`] reads it on the next render. The next action
    /// overwrites it, which is the lean v1 expiry.
    ///
    /// `action` is the already-localized button label; `name` is the target
    /// service/entry name. On `Err`, only the first non-empty line
    /// of the systemctl/stderr blob is kept and trimmed (it is often a
    /// multi-line "See 'systemctl status ...'" blob) so the status line stays a
    /// single line.
    pub fn from_result(res: &Result<(), String>, action: &'static str, name: &str) -> Self {
        match res {
            Ok(()) => ActionFeedback {
                is_error: false,
                text: format_action_feedback(
                    i18n::t("feedback.action_succeeded"),
                    action,
                    name,
                    None,
                ),
            },
            Err(e) => {
                let first = e
                    .lines()
                    .map(str::trim)
                    .find(|line| !line.is_empty())
                    .unwrap_or("");
                let text = if first.is_empty() {
                    format_action_feedback(i18n::t("feedback.action_failed"), action, name, None)
                } else {
                    format_action_feedback(
                        i18n::t("feedback.action_failed_detail"),
                        action,
                        name,
                        Some(first),
                    )
                };
                ActionFeedback {
                    is_error: true,
                    text,
                }
            }
        }
    }

    /// `true` if the action failed (the status line then picks the error color).
    pub fn is_error(&self) -> bool {
        self.is_error
    }

    /// The rendered (single-line) status message.
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// Apply the shared action-feedback placeholders without coupling backend
/// providers to a locale or sentence order. New platform providers can return
/// the same `Result` and reuse this UI contract unchanged.
fn format_action_feedback(
    template: &str,
    action: &str,
    target: &str,
    detail: Option<&str>,
) -> String {
    template
        .replace("{action}", action)
        .replace("{target}", target)
        .replace("{detail}", detail.unwrap_or(""))
}

// ── FilterSpec (Copy contract for a status-filter enum) ───────────────────────

/// `Copy` contract a status-filter enum implements so the generic
/// [`filter_pill_row`] / `filter_pill` helpers can render it. Implemented by
/// `services_view::ServiceFilter` and `startup_view::StartupFilter`.
///
/// * `label` — the human-facing pill text ("All" / "Active" / ...).
/// * `id` — a stable `&'static str` discriminator ALSO used as the
///   `Hover::Static` payload so the active/hover overlay resolves correctly
///   (the same id round-trips through `RootView.set_hover` on hover and is
///   matched back here on the next render).
///
/// Requires `PartialEq` (so the active pill can be detected via `this ==
/// active`) and `'static` (so pill `on_click` closures capturing an `F` stay
/// `'static`).
pub trait FilterSpec: Copy + PartialEq + 'static {
    /// Human-facing pill text for this filter value (e.g. "All" / "Active").
    fn label(self) -> &'static str;
    /// Stable `&'static str` discriminator, also reused as the `Hover::Static`
    /// payload so the active/hover overlay round-trips through `RootView.set_hover`.
    fn id(self) -> &'static str;
    /// Optional leading glyph rendered inside the pill (e.g. Active → CircleCheck,
    /// Failed → TriangleAlert). Defaults to `None` (label-only pill) so existing
    /// impls keep their text-only look without overriding this. Implementors
    /// override it ONLY where a fitting `IconId` exists; `filter_pill` threads it
    /// into the pill's leading slot. There is no semantic id for "all"/every
    /// status, so the catch-all filter stays text.
    fn icon(self) -> Option<IconId> {
        None
    }
}

// ── status-filter pill row ────────────────────────────────────────────────────

/// Build the status-filter pill cluster (a row of pills, one per entry in
/// `pills`). Generic over `F: FilterSpec` (services' / startup's filter enum).
///
/// `active` is the currently-selected value (its pill renders as active);
/// `hovered` is the current `RootView` hover slot (for the overlay); `entity`
/// is the RootView entity, captured (cloned) into each pill's `on_click` /
/// `on_hover` closures so the pill stays decoupled from `root`.
///
/// `on_select` receives the chosen `F` and a `&mut RootView`; the helper calls
/// `cx.notify()` after `on_select` returns, so the closure only needs to write
/// the field — e.g. `|f, v| v.services_state.filter = f`. The closure must be
/// `Clone` because one is moved into each pill's element tree (the non-capturing
/// closures used at the call sites satisfy this trivially).
pub fn filter_pill_row<F: FilterSpec>(
    theme: &Theme,
    pills: &[F],
    active: F,
    hovered: Option<&Hover>,
    entity: &Entity<RootView>,
    on_select: impl Fn(F, &mut RootView) + Clone + 'static,
) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_4,
        ))
        .children(pills.iter().map(|&f| {
            // Clone the per-view on_select into each pill (the outer closure is
            // borrowed by the `.map` adapter; each pill needs its own `'static`
            // copy for its element tree).
            let on_select = on_select.clone();
            filter_pill(theme, f, f == active, hovered, entity, on_select)
        }))
}

/// One status-filter pill. Built as the own `taskmanager_ui` pill visual inside
/// our focusable shell — the SAME structure [`elements::pill`] now uses (active
/// = accent fill + on-accent text, idle = surface + border), with the
/// `FilterSpec` icon layered in as a leading glyph when one exists. The shell
/// is a real tab stop with the accent focus ring, and gpui's
/// `ClickEvent::Keyboard` fires Enter/Space activation for focused elements
/// with click listeners — the keyboard contract the gc Button provided. The
/// `on_click` closure runs the caller-supplied `on_select` under
/// `entity.update` (so it executes under `&mut RootView` + `&mut Context<RootView>`
/// exactly like the old in-view copies), then notifies; the `on_hover` closure
/// publishes `Hover::Static(id_str)` to RootView — identical for every page,
/// hence shared.
///
/// `hovered` is intentionally unused (the `_hovered` param [`elements::pill`]
/// took was already a legacy no-op): the hover overlay is the shell's native
/// behavior, and the cursor tooltip is driven by the `on_hover` closure below.
/// Kept in the signature so [`filter_pill_row`] can forward it without a
/// special-case call site.
fn filter_pill<F: FilterSpec>(
    theme: &Theme,
    this: F,
    is_active: bool,
    _hovered: Option<&Hover>,
    entity: &Entity<RootView>,
    on_select: impl Fn(F, &mut RootView) + 'static,
) -> impl IntoElement {
    let id_str = this.id();
    let label = this.label();
    let ent_c = entity.clone();
    let ent_h = entity.clone();
    // Same shell as elements::pill: label + on_click + on_hover + focus ring.
    let wrapper = div()
        .id(id_str)
        .focusable()
        .tab_stop(true)
        .cursor_pointer()
        .focus(elements::focus_ring(theme))
        .on_click(move |_ev, _win, cx: &mut App| {
            ent_c.update(cx, |v, cx| {
                on_select(this, v);
                cx.notify();
            });
        })
        .on_hover(move |is_hov: &bool, _win: &mut Window, cx: &mut App| {
            ent_h.update(cx, |v, cx| {
                v.set_hover(
                    if *is_hov {
                        Some(Hover::Static(id_str))
                    } else {
                        None
                    },
                    cx,
                );
            });
        });
    let pill = Pill::new(
        label,
        if is_active {
            PillState::Active
        } else {
            PillState::Idle
        },
        theme.palette(),
    );
    // The optional leading glyph (None → label-only, the historical look).
    match this.icon() {
        Some(ic) => wrapper
            .flex()
            .items_center()
            .gap(taskmanager_ui::theme_binding::definite_length(
                tokens::SPACE_6,
            ))
            .child(taskmanager_ui::icons_binding::icon(ic).size(px(12.0)))
            .child(pill),
        None => wrapper.child(pill),
    }
}

// ── search box ───────────────────────────────────────────────────────────────

/// The recurring list-page search box: the own [`TextInput`] shell bound to
/// the persistent `Entity<TextInputState>` held by the owning `RootView`. The
/// `InputEvent::Change` subscription set up at entity-creation
/// time (each view's `init_search_entity`) mirrors the value into the page's
/// `RootView.<page>_state.query`, so this helper only owns the visual shell.
///
/// Escape clears the field (guarded, so repeated renders do not re-notify);
/// the magnifier glyph keeps the historical look; the field fills the fixed
/// 280px shell width.
pub fn search_box(palette: &Palette, search_entity: &Entity<TextInputState>) -> impl IntoElement {
    search_box_sized(palette, search_entity, 280.0)
}

/// Sized variant of [`search_box`]. Page-specific typed layout projections own
/// the width choice; this primitive has no second responsive-mode authority.
pub fn search_box_sized(
    palette: &Palette,
    search_entity: &Entity<TextInputState>,
    width: f32,
) -> Div {
    div()
        .flex()
        .items_center()
        .w(px(width))
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_8,
        ))
        .debug_selector(|| "tm-search-box".to_string())
        .child(
            taskmanager_ui::icons_binding::icon(IconId::Search)
                .size(px(14.0))
                .text_color(taskmanager_ui::theme_binding::hsla(palette.fg_muted)),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .child(TextInput::new(search_entity.clone(), *palette).height(24.0)),
        )
}

// ── empty state ──────────────────────────────────────────────────────────────

/// Centered, localized "no rows" hint shown in place of the Table when a
/// filtered list is empty. `noun` is already resolved from the locale catalog;
/// `query` is echoed only when a filter is active. Fills the available vertical
/// space (`flex_1`) so the layout does not collapse to the controls row.
pub fn empty_state(theme: &Theme, noun: &str, query: &str) -> AnyElement {
    let icon = empty_state_icon(query);
    let msg = if query.is_empty() {
        i18n::t("empty.none").replace("{noun}", noun)
    } else {
        i18n::t("empty.no_match")
            .replace("{noun}", noun)
            .replace("{query}", query)
    };
    state_illustration(theme, icon, &msg, None, theme.accent).into_any_element()
}

fn empty_state_icon(query: &str) -> IconId {
    if query.is_empty() {
        IconId::Applications
    } else {
        IconId::Search
    }
}

/// Shared visual grammar for empty and unavailable states: a quiet icon tile,
/// readable title, and optional detail line. This keeps Services, Startup,
/// Users, and future list pages from falling back to unrelated text-only
/// placeholders while preserving the honest state-specific copy.
fn state_illustration(
    theme: &Theme,
    icon: IconId,
    title: &str,
    detail: Option<&str>,
    tone: taskmanager_theme::Color,
) -> Div {
    let panel = StatePanel::new(icon, title.to_owned(), theme.palette()).tone(tone);
    if let Some(detail) = detail {
        panel.detail(detail.to_owned()).render()
    } else {
        panel.render()
    }
}

/// What an empty filtered list actually means: a genuine "no items", a search
/// with no matches, or a source that FAILED before it could answer. An empty
/// list from an `Unavailable` / `Partial` source must never read as "No
/// services" — the failure is real state, and the views render distinct copy
/// for it. Returns the typed failure when the empty list is caused by the
/// source (and the user is not filtering, where "no match" stays the honest
/// answer); `None` means a normal empty list.
pub fn empty_state_failure(sources: &[SourceStatus], has_query: bool) -> Option<FailureKind> {
    if has_query {
        return None;
    }
    merge_source_lines(sources).map(|merged| merged.notice.failure())
}

/// Centered "data unavailable" state for a failed source: the typed reason
/// (localized via the failure-copy table, e.g. "Required tool missing") under
/// an honest header — the list is empty because the platform could not
/// answer, not because nothing exists.
pub fn unavailable_state(theme: &Theme, reason: &str) -> AnyElement {
    state_illustration(
        theme,
        IconId::TriangleAlert,
        i18n::t("empty.unavailable_title"),
        Some(reason),
        theme.warning,
    )
    .into_any_element()
}

/// Build the typed empty state for an unavailable/partial source. A retry is
/// offered only when the shared provider policy says a refresh can help;
/// permission, missing-dependency, unsupported, and identity failures keep a
/// truthful explanation instead of a dead retry loop.
pub fn unavailable_source_state(
    theme: &Theme,
    sources: &[SourceStatus],
    has_query: bool,
    request: RefreshRequest,
    retry_button: &gpui::Entity<ButtonState>,
    root: &gpui::Entity<RootView>,
) -> Option<AnyElement> {
    if has_query {
        return None;
    }
    let notice = merge_source_lines(sources)?.notice;
    let mut content = state_illustration(
        theme,
        IconId::TriangleAlert,
        i18n::t("empty.unavailable_title"),
        Some(crate::gpui_app::root::platform_lists::control_error_detail(
            notice.failure(),
        )),
        theme.warning,
    );
    if notice.is_retryable() {
        content = content.child(source_retry_button(theme, request, retry_button, root));
    } else {
        content = content.child(
            div()
                .max_w(px(360.0))
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_12))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
                .child(i18n::t("source.retry_after_change")),
        );
    }
    Some(content.into_any_element())
}

/// Kind → banner-title key: the ONLY outcome→copy fold on this surface. A
/// degraded source (partial answer) keeps the softer title; stale and failed
/// sources both mean "the provider did not answer" and share the unavailable
/// title. Copy itself stays in the locale catalog.
fn banner_title_key(kind: SourceStateKind) -> &'static str {
    if kind == SourceStateKind::Degraded {
        "source.partial_title"
    } else {
        // Stale | Failed (merge never headlines a healthy/unknown line).
        "source.unavailable_title"
    }
}

/// Compact, non-blocking source warning shown above a list that still has
/// usable rows. It keeps the data visible while making partial results and
/// their recovery policy explicit.
pub fn source_notice(
    theme: &Theme,
    sources: &[SourceStatus],
    request: RefreshRequest,
    retry_button: &gpui::Entity<ButtonState>,
    root: &gpui::Entity<RootView>,
) -> Option<AnyElement> {
    source_notice_with_detail(theme, sources, request, retry_button, root, None)
}

/// Compact source warning with an optional page-specific explanation of which
/// fields are affected. The generic title and recovery policy stay shared;
/// pages add detail only when their source graph can name the missing facet.
pub fn source_notice_with_detail(
    theme: &Theme,
    sources: &[SourceStatus],
    request: RefreshRequest,
    retry_button: &gpui::Entity<ButtonState>,
    root: &gpui::Entity<RootView>,
    detail: Option<&str>,
) -> Option<AnyElement> {
    source_notice_with_detail_presentation(
        theme,
        sources,
        request,
        retry_button,
        root,
        detail,
        SourceNoticePresentation::Standard,
    )
}

/// Vertical allocation for a source notice. This is a presentation fact, not
/// a proxy for source severity: both variants render the same typed failure
/// and recovery action. `Compact` merely folds the title and failure into one
/// line so a constrained list page cannot sacrifice its primary table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceNoticePresentation {
    Standard,
    Compact,
}

/// Render a source notice within an explicit page allocation.
pub fn source_notice_with_detail_presentation(
    theme: &Theme,
    sources: &[SourceStatus],
    request: RefreshRequest,
    retry_button: &gpui::Entity<ButtonState>,
    root: &gpui::Entity<RootView>,
    detail: Option<&str>,
    presentation: SourceNoticePresentation,
) -> Option<AnyElement> {
    let merged = merge_source_lines(sources)?;
    let notice = merged.notice;
    let title = i18n::t(banner_title_key(merged.kind));
    let failure = crate::gpui_app::root::platform_lists::control_error_detail(notice.failure());
    let content = match presentation {
        SourceNoticePresentation::Standard => div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(taskmanager_ui::theme_binding::definite_length(
                tokens::SPACE_2,
            ))
            .child(
                div()
                    .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_12))
                    .text_color(taskmanager_ui::theme_binding::hsla(theme.fg))
                    .child(title.to_string()),
            )
            .child(
                div()
                    .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_11))
                    .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
                    .child(failure.to_string()),
            )
            .children(detail.into_iter().map(|detail| {
                div()
                    .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_11))
                    .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
                    .child(detail.to_string())
            })),
        SourceNoticePresentation::Compact => div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(taskmanager_ui::theme_binding::definite_length(
                tokens::SPACE_1,
            ))
            // Each truncating line is wrapped in a flex-row (the title-row
            // pattern): bare column-child `truncate()` poisons gpui's nowrap
            // measure cache and clips the line hard at narrow widths.
            .child(
                div().flex().flex_row().min_w(px(0.0)).child(
                    crate::gpui_app::elements::truncated_text(&format!("{title} · {failure}"))
                        .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_11))
                        .text_color(taskmanager_ui::theme_binding::hsla(theme.fg)),
                ),
            )
            .children(detail.into_iter().map(|detail| {
                div().flex().flex_row().min_w(px(0.0)).child(
                    crate::gpui_app::elements::truncated_text(detail)
                        .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_11))
                        .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim)),
                )
            })),
    };
    let mut banner = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(taskmanager_ui::theme_binding::definite_length(
            match presentation {
                SourceNoticePresentation::Standard => tokens::SPACE_8,
                SourceNoticePresentation::Compact => tokens::SPACE_4,
            },
        ))
        .px(taskmanager_ui::theme_binding::definite_length(
            match presentation {
                SourceNoticePresentation::Standard => tokens::SPACE_10,
                SourceNoticePresentation::Compact => tokens::SPACE_6,
            },
        ))
        .py(taskmanager_ui::theme_binding::definite_length(
            match presentation {
                SourceNoticePresentation::Standard => tokens::SPACE_6,
                SourceNoticePresentation::Compact => tokens::SPACE_3,
            },
        ))
        .rounded(px(6.0))
        .bg(taskmanager_ui::theme_binding::fill(
            theme.warning.with_alpha(0.10),
        ))
        .border_1()
        .border_color(taskmanager_ui::theme_binding::hsla(
            theme.warning.with_alpha(0.28),
        ))
        .child(
            taskmanager_ui::icons_binding::icon(IconId::TriangleAlert)
                .size(px(14.0))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.warning)),
        )
        .child(content);
    if notice.is_retryable() {
        banner = banner.child(source_retry_button(theme, request, retry_button, root));
    } else {
        banner = banner.child(
            div()
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_11))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
                .child(i18n::t("source.retry_after_change")),
        );
    }
    Some(banner.into_any_element())
}

fn source_retry_button(
    theme: &Theme,
    request: RefreshRequest,
    state: &gpui::Entity<ButtonState>,
    root: &gpui::Entity<RootView>,
) -> Button {
    let root = root.clone();
    Button::new(state.clone(), theme.palette())
        .variant(ButtonVariant::Secondary)
        .icon(IconId::Refresh)
        .label(i18n::t("common.refresh"))
        .on_activate(move |_event, _window, cx| {
            root.update(cx, |view, cx| {
                view.request_refresh(request);
                cx.notify();
            });
        })
}

#[cfg(test)]
#[path = "../../tests/gui/gpui_gpui_app_list_view_empty_state_tests.rs"]
mod empty_state_tests;

// ── action-bar status line ───────────────────────────────────────────────────

/// The action bar's trailing status line. When `feedback` is present it takes
/// the line over (error → `theme.gpu`, success → `theme.disk`, echoing the
/// feedback text); otherwise `selected_hint` is shown in `theme.fg_dim`.
///
/// `selected_hint` is the caller-resolved selection hint (e.g.
/// `format!("Selected {}", name)` when a row is selected, else `"Select a
/// service"` / `"Select a startup entry"`) — each page owns its own hint text.
/// The colors echo the status badges in the list rows (Failed/error →
/// `theme.gpu`, Active/Enabled → `theme.disk`) so the line reads as one family
/// with the status column.
pub fn feedback_status_line(
    theme: &Theme,
    feedback: Option<&ActionFeedback>,
    selected_hint: &str,
) -> impl IntoElement {
    let (text, color) = match feedback {
        Some(fb) if fb.is_error() => (fb.text().to_string(), theme.gpu),
        Some(fb) => (fb.text().to_string(), theme.disk),
        None => (selected_hint.to_string(), theme.fg_dim),
    };
    div()
        .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_12))
        .text_color(taskmanager_ui::theme_binding::hsla(color))
        .child(text)
}

#[cfg(test)]
#[path = "../../tests/gui/gpui_gpui_app_list_view_tests.rs"]
mod tests;
