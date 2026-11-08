// test-intent: behavior
//
// Behavior tests for the primitives component family: the tone/token
// mapping seams (badge fills with luminance-picked foregrounds, the
// four-state panel grammar, the source → panel-state wrapper mapping) and
// the progress resolution seam that keeps unavailable data from
// masquerading as a measured value — plus a construction sweep across
// skins and modes that exercises the geometry/style paths.

use super::*;
use crate::ui::components::{
    key_value_rows, message_panel, source_panel_state, source_state_panel, titled_card,
};
use taskforest_core::core::failure::FailureKind;
use taskforest_core::core::identity::ProviderId;
use taskforest_core::core::source::{SourceOutcome, SourceStatus};
use taskmanager_application::{RefreshRequest, SourceStateKind, merge_source_lines};

use taskmanager_theme::ResolvedFonts;
use taskmanager_theme::color::on_accent;
use taskmanager_theme::{HighContrast, LightDark, Skin};
use zhuge_ui_tokens::Rgba;

fn contrast_ratio(first: Color, second: Color) -> f32 {
    Rgba::new(first.r, first.g, first.b, first.a)
        .contrast_ratio(Rgba::new(second.r, second.g, second.b, second.a))
}

#[test]
fn segmented_fills_round_only_the_visible_track_ends() {
    let color = iced::Color::BLACK;
    let left = segmented_fill_style(color, 5.0, true, false).border.radius;
    let middle = segmented_fill_style(color, 5.0, false, false).border.radius;
    let right = segmented_fill_style(color, 5.0, false, true).border.radius;
    let single = segmented_fill_style(color, 5.0, true, true).border.radius;
    assert_eq!((left.top_left, left.top_right), (5.0, 0.0));
    assert_eq!((middle.bottom_left, middle.bottom_right), (0.0, 0.0));
    assert_eq!((right.bottom_left, right.bottom_right), (0.0, 5.0));
    assert_eq!((single.top_left, single.bottom_right), (5.0, 5.0));
}

fn theme_for(skin: Skin, mode: LightDark) -> Theme {
    Theme::build(
        skin,
        mode,
        HighContrast::Off,
        ResolvedFonts::system_for(skin),
    )
}

fn status(outcome: SourceOutcome) -> SourceStatus {
    SourceStatus {
        provider: ProviderId::borrowed("test.primitives"),
        outcome,
        item_count: 0,
    }
}

/// Every badge tone fills from its palette semantic token and picks its
/// foreground by fill luminance — and that pick is the WCAG black/white
/// choice, which never drops below the 4.5:1 text band over any skin or mode.
#[test]
fn badge_tones_map_onto_palette_semantics_with_contrast_foregrounds() {
    let theme = theme_for(Skin::Gnome, LightDark::Dark);
    let palette = theme.palette();
    assert_eq!(BadgeTone::Warning.fill(&theme), palette.warning);
    assert_eq!(BadgeTone::Danger.fill(&theme), palette.danger);
    assert_eq!(BadgeTone::Accent.fill(&theme), palette.accent);

    let tinted = [BadgeTone::Warning, BadgeTone::Danger, BadgeTone::Accent];
    for skin in Skin::ALL {
        for mode in LightDark::ALL {
            let theme = theme_for(skin, mode);
            for tone in tinted {
                let fill = tone.fill(&theme);
                let foreground = tone.foreground(&theme);
                assert_eq!(foreground, on_accent(fill));
                assert!(
                    contrast_ratio(foreground, fill) >= 4.5,
                    "{skin:?}/{mode:?}/{tone:?}: {}",
                    contrast_ratio(foreground, fill)
                );
            }
        }
    }
}

/// Unavailable progress resolves to the Unknown grammar — never to a
/// measured value, and specifically never to a disguised 0% — while real
/// measurements (including a true measured zero) clamp into 0..=1.
#[test]
fn unavailable_progress_never_resolves_to_a_measured_value() {
    assert_eq!(progress_fill(None), ProgressFill::Unknown);
    assert_ne!(progress_fill(None), ProgressFill::Determinate(0.0));
    assert_eq!(progress_fill(Some(f32::NAN)), ProgressFill::Unknown);
    assert_eq!(progress_fill(Some(f32::INFINITY)), ProgressFill::Unknown);
    assert_eq!(
        progress_fill(Some(f32::NEG_INFINITY)),
        ProgressFill::Unknown
    );
    assert_eq!(progress_fill(Some(0.0)), ProgressFill::Determinate(0.0));
    assert_eq!(progress_fill(Some(0.42)), ProgressFill::Determinate(0.42));
    assert_eq!(progress_fill(Some(-3.0)), ProgressFill::Determinate(0.0));
    assert_eq!(progress_fill(Some(1.9)), ProgressFill::Determinate(1.0));
}

/// The state grammar owns a fixed tone mapping per state: quiet muted for
/// empty, failure for unanswered, caution for partial — constant across
/// every skin and mode.
#[test]
fn panel_states_map_onto_their_palette_tone_grammar() {
    for skin in Skin::ALL {
        for mode in LightDark::ALL {
            let theme = theme_for(skin, mode);
            let palette = theme.palette();
            assert_eq!(PanelState::Empty.tone(&theme), palette.fg_muted);
            assert_eq!(PanelState::Unavailable.tone(&theme), palette.danger);
            assert_eq!(PanelState::Partial.tone(&theme), palette.warning);
        }
    }
}

/// The source wrapper keeps its original semantics at the new seam: a
/// degraded merged source (rows still usable) maps to Partial with the
/// partial copy, every unanswered source maps to Unavailable with the
/// unavailable copy, and the copy-key choice is unchanged.
#[test]
fn source_wrapper_maps_merged_kinds_onto_the_shared_panel_grammar() {
    let degraded = status(SourceOutcome::Partial(FailureKind::MissingDependency));
    let merged = merge_source_lines(&[degraded]).expect("degraded source merges");
    assert_eq!(merged.kind, SourceStateKind::Degraded);
    assert_eq!(source_panel_state(merged.kind), PanelState::Partial);
    assert_eq!(merged.kind.banner_title_key(), "source.partial_title");

    let failed = status(SourceOutcome::Unavailable(FailureKind::PermissionDenied));
    let merged = merge_source_lines(&[failed]).expect("failed source merges");
    assert_eq!(merged.kind, SourceStateKind::Failed);
    assert_eq!(source_panel_state(merged.kind), PanelState::Unavailable);
    assert_eq!(merged.kind.banner_title_key(), "source.unavailable_title");

    for kind in [
        SourceStateKind::Ok,
        SourceStateKind::Unknown,
        SourceStateKind::Stale,
        SourceStateKind::Failed,
    ] {
        assert_eq!(source_panel_state(kind), PanelState::Unavailable);
    }
    assert_eq!(
        source_panel_state(SourceStateKind::Degraded),
        PanelState::Partial
    );
}

/// The wrappers stay construction-compatible: the empty-state wrapper builds
/// from a localized message, a degraded source still produces a retryable
/// panel, and absent sources still produce NO panel (an absent source never
/// fabricates a failure surface).
#[test]
fn message_panel_wrapper_builds_and_absent_sources_stay_absent() {
    let theme = theme_for(Skin::Gnome, LightDark::Dark);
    let _ = message_panel(&theme, "waiting");
    let _ = titled_card(&theme, "card", text("body"));
    assert!(source_state_panel(&theme, None, RefreshRequest::All).is_none());

    let degraded = [status(SourceOutcome::Partial(
        FailureKind::MissingDependency,
    ))];
    assert!(source_state_panel(&theme, Some(&degraded), RefreshRequest::All).is_some());
}

/// Every primitive constructs without I/O across every skin and mode, in
/// its normal and degenerate shapes (non-finite progress samples, every
/// tone, every panel state with and without detail/action) — the sweep
/// exercises the token reads and FillPortion geometry where a broken token
/// binding would otherwise panic in a real frame.
#[test]
fn primitives_build_across_skins_modes_and_degenerate_states() {
    let tones = [BadgeTone::Warning, BadgeTone::Danger, BadgeTone::Accent];
    let states = [
        PanelState::Empty,
        PanelState::Unavailable,
        PanelState::Partial,
    ];
    for skin in Skin::ALL {
        for mode in LightDark::ALL {
            let theme = theme_for(skin, mode);
            for tone in tones {
                let _ = badge(&theme, "label", tone);
                let _ = progress(&theme, None, tone);
                let _ = progress(&theme, Some(0.5), tone);
                let _ = progress(&theme, Some(f32::NAN), tone);
            }
            let _ = divider(&theme);
            let _ = tooltip(&theme, text("trigger").into(), "a tip");
            for state in states {
                let _ = state_panel(&theme, state, IconId::TriangleAlert, "title", None, None);
                let _ = state_panel(
                    &theme,
                    state,
                    IconId::Applications,
                    "title",
                    Some("detail".to_string()),
                    Some(text("act").into()),
                );
            }
        }
    }
}

// ── Measured geometry equivalence across the shared-kit hand-off ──────────
//
// `state_panel` now builds on `zhuge_ui_kit_iced::surface::state_panel`, which
// owns the padded, centered, styled surface and appends the panel's content to
// its `action` slot. These tests lay both constructions out with Iced's
// headless software renderer and compare what the renderer actually sees: the
// panel extent, the painted surfaces, and every visible copy fragment with its
// bounds. A phantom row from the kit's mandatory title/message slots, a lost
// icon tile, or a shifted gutter fails here on measured geometry rather than
// on source text.

use iced::Rectangle;
use iced::advanced::text::IntoFragment;
use iced::widget::{column, container, text};
use zhuge_ui_kit_iced::surface::StatePanelGeometry;

#[derive(Clone, Debug)]
struct Snapshot {
    extent: iced::Size,
    containers: Vec<Rectangle>,
    fragments: Vec<(String, Rectangle)>,
}

#[derive(Default)]
struct SnapshotCollector {
    containers: Vec<Rectangle>,
    fragments: Vec<(String, Rectangle)>,
}

impl iced::advanced::widget::Operation for SnapshotCollector {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn iced::advanced::widget::Operation)) {
        operate(self);
    }

    fn container(&mut self, _id: Option<&iced::advanced::widget::Id>, bounds: Rectangle) {
        self.containers.push(bounds);
    }

    fn text(&mut self, _id: Option<&iced::advanced::widget::Id>, bounds: Rectangle, text: &str) {
        self.fragments.push((text.to_owned(), bounds));
    }
}

/// The surfaces and copy fragments one element reports at an 800x600 slot.
fn snapshot(element: &mut IcedElement<'_>) -> Snapshot {
    use iced::Size;
    use iced::advanced::layout::Limits;
    use iced::advanced::renderer::Headless;
    use iced::advanced::widget::Tree;
    use taskmanager_assets::embedded_fonts;

    {
        let mut fonts = iced::advanced::graphics::text::font_system()
            .write()
            .expect("font system");
        for font in embedded_fonts() {
            fonts.load_font(font);
        }
    }
    let renderer = iced::futures::executor::block_on(iced::Renderer::new(
        crate::theme_binding::BUNDLED_UI_FONT,
        iced::Pixels(16.0),
        Some("tiny-skia"),
    ))
    .expect("software renderer");

    let mut tree = Tree::new(&*element);
    let node = element.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &Limits::new(Size::ZERO, Size::new(800.0, 600.0)),
    );
    let mut collector = SnapshotCollector::default();
    element.as_widget_mut().operate(
        &mut tree,
        iced::advanced::Layout::new(&node),
        &renderer,
        &mut collector,
    );
    Snapshot {
        extent: node.size(),
        containers: collector.containers,
        fragments: collector.fragments,
    }
}

/// The surface the local construction built before the kit owned the layout
/// container — the geometry oracle the kit-backed panel is measured against.
fn oracle_state_panel<'a>(
    theme: &'a Theme,
    state: PanelState,
    icon: IconId,
    title: &'a str,
    detail: Option<String>,
    action: Option<IcedElement<'a>>,
) -> IcedElement<'a> {
    let palette = theme.palette();
    let tone = state.tone(theme);
    let tone_fill = crate::theme_binding::color(tone);
    let tile_fill = crate::theme_binding::color(tone.with_alpha(0.12));
    let tile_border = crate::theme_binding::color(tone.with_alpha(0.30));
    let muted = crate::theme_binding::color(palette.fg_muted);
    let icon_extent = f32::from(tokens::UiSize::Small.icon_size());
    let surface = theme;

    let mut content = iced::widget::column![].spacing(f32::from(tokens::SPACE_8));
    content = content.push(
        container(crate::icons::icon(theme, icon, icon_extent))
            .width(Length::Fixed(STATE_TILE))
            .height(Length::Fixed(STATE_TILE))
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(move |_| bordered(tile_border, (STATE_TILE / 2.0).into(), tile_fill)),
    );
    content = content.push(text(title).size(f32::from(tokens::FONT_BODY)).color(muted));
    if let Some(detail) = detail {
        content = content.push(
            text(detail)
                .size(f32::from(tokens::FONT_HEADER))
                .color(muted),
        );
    }
    if let Some(action) = action {
        content = content.push(action);
    }

    container(content.width(Length::Fill).align_x(Alignment::Center))
        .style(move |_| state_panel_style(surface, tone_fill))
        .padding(f32::from(tokens::SPACE_16))
        .center_x(Length::Fill)
        .width(Length::Fill)
        .into()
}

fn same_bounds(a: &Rectangle, b: &Rectangle) -> bool {
    (a.x - b.x).abs() < 0.001
        && (a.y - b.y).abs() < 0.001
        && (a.width - b.width).abs() < 0.001
        && (a.height - b.height).abs() < 0.001
}

fn describe(list: &[Rectangle]) -> Vec<(f32, f32, f32, f32)> {
    list.iter().map(|r| (r.x, r.y, r.width, r.height)).collect()
}

/// The surfaces that carry a box, deduplicated: the kit's own column wrapper
/// reports the same rect as the surface it centers, and iced emits a container
/// operation for it. Painted geometry, not widget count, is the contract.
fn painted_surfaces(snap: &Snapshot) -> Vec<Rectangle> {
    let mut seen: Vec<Rectangle> = Vec::new();
    for rect in &snap.containers {
        if !seen.iter().any(|known| same_bounds(known, rect)) {
            seen.push(*rect);
        }
    }
    seen.sort_by(|a, b| {
        (a.x, a.y, a.width, a.height)
            .partial_cmp(&(b.x, b.y, b.width, b.height))
            .expect("finite rect coordinates")
    });
    seen
}

/// The copy a user can read: non-empty fragments, in document order.
fn visible_copy(snap: &Snapshot) -> Vec<(String, Rectangle)> {
    snap.fragments
        .iter()
        .filter(|(text, _)| !text.is_empty())
        .cloned()
        .collect()
}

fn assert_same_geometry(case: &str, oracle: &Snapshot, kit: &Snapshot) {
    assert!(
        (oracle.extent.width - kit.extent.width).abs() < 0.001
            && (oracle.extent.height - kit.extent.height).abs() < 0.001,
        "{case}: panel extent {:?} vs {:?}",
        oracle.extent,
        kit.extent
    );

    let oracle_surfaces = painted_surfaces(oracle);
    let kit_surfaces = painted_surfaces(kit);
    assert_eq!(
        describe(&oracle_surfaces),
        describe(&kit_surfaces),
        "{case}: the panel's painted surfaces drifted"
    );

    let oracle_copy = visible_copy(oracle);
    let kit_copy = visible_copy(kit);
    assert_eq!(
        oracle_copy
            .iter()
            .map(|(text, _)| text.clone())
            .collect::<Vec<_>>(),
        kit_copy
            .iter()
            .map(|(text, _)| text.clone())
            .collect::<Vec<_>>(),
        "{case}: the panel's visible copy changed"
    );
    for ((text, here), (_, there)) in oracle_copy.iter().zip(kit_copy.iter()) {
        assert!(
            same_bounds(here, there),
            "{case}: {text:?} renders at {here:?} in the oracle and {there:?} in the kit panel"
        );
    }
}

/// The kit's neutral rows may not push copy around either: the whole panel is
/// measured with and without the optional detail and action slots.
#[test]
fn the_kit_backed_state_panel_keeps_the_measured_geometry() {
    let theme = theme_for(Skin::Gnome, LightDark::Dark);

    for (case, state, detail, with_action) in [
        ("empty", PanelState::Empty, None, false),
        ("empty-with-action", PanelState::Empty, None, true),
        (
            "unavailable",
            PanelState::Unavailable,
            Some("detail"),
            false,
        ),
        (
            "unavailable-full",
            PanelState::Unavailable,
            Some("detail"),
            true,
        ),
        ("partial", PanelState::Partial, Some("partial detail"), true),
    ] {
        let mut kit_panel = state_panel(
            &theme,
            state,
            IconId::TriangleAlert,
            "title",
            detail.map(str::to_owned),
            with_action.then(|| text("act".to_string()).into()),
        );
        let mut oracle_panel = oracle_state_panel(
            &theme,
            state,
            IconId::TriangleAlert,
            "title",
            detail.map(str::to_owned),
            with_action.then(|| text("act".to_string()).into()),
        );
        let kit = snapshot(&mut kit_panel);
        let oracle = snapshot(&mut oracle_panel);
        assert_same_geometry(case, &oracle, &kit);
    }
}

/// The titled card the product spelled itself before the kit owned the layout:
/// a `column![title, body]` inside a padded container, with the title at the
/// body role's type size and the gutter at the spacing token.
///
/// This is the geometry oracle the kit-backed card is measured against — the
/// same shape the state panel uses. If the owner ever drifts from what the
/// product rendered, this fails instead of silently re-spacing every card.
fn oracle_titled_card<'a>(
    theme_snapshot: &'a Theme,
    title: impl IntoFragment<'a>,
    body: impl Into<IcedElement<'a>>,
) -> IcedElement<'a> {
    container(
        column![text(title).size(f32::from(tokens::FONT_BODY)), body.into()]
            .spacing(f32::from(tokens::SPACE_6)),
    )
    .style(move |_| theme::card_style(theme_snapshot))
    .padding(f32::from(tokens::SPACE_10))
    .width(Length::Fill)
    .into()
}

/// The titled card now binds `zhuge_ui_kit_iced::surface::titled_card`. The
/// geometry the product rendered before the hand-off must survive it exactly,
/// for the body shapes the pages actually pass: a key/value group whose rows
/// fill, a plain scrollable column, and a nested card.
#[test]
fn the_kit_backed_titled_card_keeps_the_measured_geometry() {
    let theme = theme_for(Skin::Gnome, LightDark::Dark);

    // Every shape below is a body some page really passes, and the set is
    // deliberately closed around the strictest one: `service_details` builds
    // `lifecycle_content` as a column that never names a width, so it is the
    // case that shows whether the owner has taken a second authority over the
    // card's content. A comparison that only used filling bodies would have
    // passed while that page re-flowed.
    let rows = || {
        key_value_rows(vec![
            ("label".to_owned(), "value".to_owned()),
            ("longer label".to_owned(), "another value".to_owned()),
        ])
    };
    let filling_column = || {
        column![text("first line"), text("second line")]
            .spacing(2)
            .width(Length::Fill)
            .into()
    };
    let nested = || {
        column![
            key_value_rows(vec![("label".to_owned(), "value".to_owned())]),
            text("an action row"),
        ]
        .spacing(6)
        .width(Length::Fill)
        .into()
    };

    // `service_details.rs`'s lifecycle group: a column with no declared width.
    let unfilled_nested = || {
        column![
            key_value_rows(vec![("label".to_owned(), "value".to_owned())]),
            text("jump to process"),
        ]
        .spacing(8)
        .into()
    };
    let plain = || {
        column![text("first line"), text("second line")]
            .spacing(2)
            .into()
    };

    let cases: [(&str, IcedElement<'static>, IcedElement<'static>); 5] = [
        ("key-value", rows(), rows()),
        ("filling-column", filling_column(), filling_column()),
        ("nested-group", nested(), nested()),
        ("unfilled-nested", unfilled_nested(), unfilled_nested()),
        ("plain-shrink", plain(), plain()),
    ];
    for (case, kit_body, oracle_body) in cases {
        let mut kit_card = titled_card(&theme, "Card Title", kit_body);
        let mut oracle_card = oracle_titled_card(&theme, "Card Title", oracle_body);
        assert_same_geometry(case, &snapshot(&mut oracle_card), &snapshot(&mut kit_card));
    }
}

/// One width authority per card. The card box spans its slot because
/// `CardGeometry::width` says so; the content column is left alone, so a body
/// decides its own width and the owner never overrules it. This is what lets a
/// page that names no width keep the region it had before the hand-off.
#[test]
fn the_card_box_fills_and_the_owner_leaves_the_content_width_to_the_body() {
    let theme = theme_for(Skin::Gnome, LightDark::Dark);
    let mut card = titled_card(&theme, "Card Title", text("body"));
    let card_snapshot = snapshot(&mut card);

    let box_quad = painted_surfaces(&card_snapshot)
        .into_iter()
        .next()
        .expect("the card paints its own box");
    assert!(
        (box_quad.width - 800.0).abs() < 0.001,
        "the card box spans the slot, not {:?}",
        box_quad.width
    );

    let mut filling = titled_card(&theme, "Card Title", text("body").width(Length::Fill));
    let filling_snapshot = snapshot(&mut filling);
    assert_eq!(
        card_snapshot.extent, filling_snapshot.extent,
        "a body naming a width must not change the card's outer extent"
    );
}

/// The card's inner padding and the title/body gutter are token-bound, so the
/// measured card must actually carry them: a card that lost its padding would
/// still pass a shape comparison against another wrong card.
#[test]
fn the_titled_card_keeps_its_token_padding_and_title_gutter() {
    let theme = theme_for(Skin::Gnome, LightDark::Dark);
    let mut card = titled_card(&theme, "Card Title", text("body"));
    let snapshot = snapshot(&mut card);

    let title = snapshot
        .fragments
        .iter()
        .find(|(text, _)| text == "Card Title")
        .map(|(_, bounds)| *bounds)
        .expect("the card renders its title");
    let body = snapshot
        .fragments
        .iter()
        .find(|(text, _)| text == "body")
        .map(|(_, bounds)| *bounds)
        .expect("the card renders its body");

    // The card's content starts one spacing token in from its edge.
    let padding = f32::from(tokens::SPACE_10);
    assert!(
        (title.x - padding).abs() < 0.001,
        "the title starts at {padding} from the card edge, not {:?}",
        title.x
    );
    // The title and the body are separated by the spacing token.
    let gutter = body.y - (title.y + title.height);
    assert!(
        (gutter - f32::from(tokens::SPACE_6)).abs() < 0.001,
        "the title/body gutter is {gutter}, not the spacing token"
    );
}

/// `message_panel` is the wrapper every loading/empty page reaches for; it must
/// keep the exact surface the shared empty-state grammar had before the hand-off.
#[test]
fn message_panel_renders_the_oracle_empty_state_geometry() {
    let theme = theme_for(Skin::Gnome, LightDark::Dark);
    let mut oracle_panel = oracle_state_panel(
        &theme,
        PanelState::Empty,
        IconId::Applications,
        "no sessions",
        None,
        None,
    );
    let mut kit_panel = message_panel(&theme, "no sessions");
    assert_same_geometry(
        "message_panel",
        &snapshot(&mut oracle_panel),
        &snapshot(&mut kit_panel),
    );
}

/// The neutralized rows carry no extent: two empty fragments sit at the panel's
/// content origin and each measures well under one logical pixel.
#[test]
fn the_kit_state_panel_materializes_no_row_the_caller_did_not_ask_for() {
    let theme = theme_for(Skin::Gnome, LightDark::Dark);
    let mut panel = message_panel(&theme, "waiting");
    let snapshot = snapshot(&mut panel);
    let neutral: Vec<f32> = snapshot
        .fragments
        .iter()
        .filter(|(text, _)| text.is_empty())
        .map(|(_, bounds)| bounds.height)
        .collect();
    assert!(
        neutral.is_empty(),
        "an absent row is not materialized: the kit laid out {} phantom rows",
        neutral.len()
    );
    let visible = visible_copy(&snapshot);
    assert_eq!(
        visible
            .iter()
            .map(|(text, _)| text.clone())
            .collect::<Vec<_>>(),
        vec!["waiting".to_owned()],
        "the panel shows only the copy its caller owns"
    );
}

/// The Applications empty state hands its one localized line to the same shared
/// kit container and keeps the product contract the kit geometry cannot state:
/// the panel surface fills the whole body slot and the message stays centered on
/// both axes. Measured against the construction the page shipped before the
/// hand-off — a shrink-to-fit kit panel on its own would sit at the top of the
/// slot with a phantom second row, which is exactly what this pins out.
#[test]
fn a_full_slot_page_surface_keeps_the_kit_panel_centered() {
    let theme = theme_for(Skin::Gnome, LightDark::Dark);
    let message = "No processes match \u{201c}zzz\u{201d}";

    let mut shipped: IcedElement<'_> = container(text(message).size(f32::from(tokens::FONT_13)))
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| crate::theme::panel_style(&theme))
        .into();

    let mut current: IcedElement<'_> = container(surface::state_panel(
        Some(message.to_owned()),
        None,
        None,
        StatePanelGeometry {
            padding: iced::Padding::ZERO,
            spacing: 0.0,
            title_size: f32::from(tokens::FONT_13),
            ..StatePanelGeometry::default()
        },
        |_| container::Style::default(),
    ))
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .width(Length::Fill)
    .height(Length::Fill)
    .style(move |_| crate::theme::panel_style(&theme))
    .into();

    let shipped = snapshot(&mut shipped);
    let current = snapshot(&mut current);
    assert!(
        (shipped.extent.height - current.extent.height).abs() < 0.001
            && (shipped.extent.width - current.extent.width).abs() < 0.001,
        "the full-slot surface changed: {:?} vs {:?}",
        shipped.extent,
        current.extent
    );
    let shipped_copy = visible_copy(&shipped);
    let current_copy = visible_copy(&current);
    assert_eq!(
        shipped_copy
            .iter()
            .map(|(text, _)| text.clone())
            .collect::<Vec<_>>(),
        current_copy
            .iter()
            .map(|(text, _)| text.clone())
            .collect::<Vec<_>>(),
        "the empty state changed its copy"
    );
    for ((text, here), (_, there)) in shipped_copy.iter().zip(current_copy.iter()) {
        assert!(
            same_bounds(here, there),
            "{text:?} renders at {here:?} shipped and {there:?} through the kit panel"
        );
    }
}
