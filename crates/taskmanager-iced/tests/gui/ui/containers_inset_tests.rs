//! Headless behavior tests for the Containers-page inset panel (ICED-046):
//! the container's own process block wears a RECESSED surface, and the
//! block's projection never fabricates members it could not enumerate.

use super::*;
use taskforest_core::core::metrics::ScalarObservation;
use taskforest_core::core::process_telemetry::{ContainerQuota, ContainerSummary};
use taskmanager_theme::color::Color;
use taskmanager_theme::color::mix;
use taskmanager_theme::fonts::ResolvedFonts;
use taskmanager_theme::theme::HighContrast;
use taskmanager_theme::theme::LightDark;
use taskmanager_theme::theme::Skin;

/// The relative luminance of one color (the sRGB coefficients, no gamma
/// expansion): enough to tell a recessed fill from the surface it sits on.
fn luminance(color: iced::Color) -> f32 {
    0.2126 * color.r + 0.7152 * color.g + 0.0722 * color.b
}

fn summary_with_members(pids: Vec<u32>) -> ContainerSummary {
    ContainerSummary {
        id: "/docker/abc".into(),
        name: "abc".into(),
        runtime: Some(IsolationKind::Docker),
        cgroup_path: "/docker/abc".into(),
        cpu_percentage: ScalarObservation::available(12.0, 1_000),
        memory_bytes: ScalarObservation::available(1_024, 1_000),
        member_pids: pids,
        // The fixture declares no cgroup ceiling.
        quota: ContainerQuota::not_declared(),
    }
}

/// The inset panel is RECESSED: its fill is darker than the row surface it
/// is cut into, on every skin. A panel that only changed the border would
/// read as an outline, not as a well.
#[test]
fn inset_panel_reads_as_a_recessed_well_inside_the_row_surface() {
    for theme in [
        Theme::dark(),
        Theme::build(
            Skin::Gnome,
            LightDark::Light,
            HighContrast::Off,
            ResolvedFonts::system_for(Skin::Gnome),
        ),
    ] {
        let inset = theme::inset_panel_style(&theme);
        let row = theme::row_style(&theme, false, false);
        let (iced::Background::Color(inset_fill), iced::Background::Color(row_fill)) = (
            inset.background.expect("the inset panel paints a fill"),
            row.background.expect("the row paints a fill"),
        ) else {
            panic!("both surfaces must paint a solid color");
        };
        assert!(
            luminance(inset_fill) < luminance(row_fill),
            "the inset fill must be darker than the surface it is cut into \
             (inset {inset_fill:?} vs row {row_fill:?})"
        );
        assert_eq!(
            inset.border.width, 1.0,
            "the recessed panel keeps the skin's hairline"
        );
    }
}

/// The inset panel keeps the skin's own colors: the fill is a blend of the
/// page's backdrop token with the theme's black token, never a literal.
#[test]
fn inset_panel_fill_is_derived_from_theme_tokens() {
    let theme = Theme::dark();
    let palette = theme.palette();
    let inset = theme::inset_panel_style(&theme);
    let iced::Background::Color(fill) = inset.background.expect("a fill") else {
        panic!("the inset panel paints a solid color");
    };
    let expected = mix(palette.window_backdrop, Color::BLACK, 0.22);
    assert!(
        (fill.r - expected.r).abs() < 1e-6
            && (fill.g - expected.g).abs() < 1e-6
            && (fill.b - expected.b).abs() < 1e-6,
        "the inset fill must come from the theme's own tokens, got {fill:?}"
    );
}

/// The member-process block lists the container's real members and counts
/// the ones it did not show.
#[test]
fn member_process_block_lists_real_members_and_counts_the_overflow() {
    let block = member_process_block(&summary_with_members(vec![10, 11, 12, 13, 14]))
        .expect("an enumerated membership projects a block");
    assert_eq!(block.labels, ["10", "11", "12"]);
    assert_eq!(block.hidden, 2, "the members beyond the bound are counted");
}

/// Membership that could not be enumerated projects NO block: the cell
/// keeps its honest dash instead of wrapping a fabricated empty list in an
/// inset panel.
#[test]
fn unenumerated_membership_projects_no_inset_block() {
    assert!(
        member_process_block(&summary_with_members(Vec::new())).is_none(),
        "an empty membership is unknown membership, never an empty process tree"
    );
}

/// The row fold keeps the dash spelling for an unenumerated membership and
/// the real count for an enumerated one.
#[test]
fn row_fold_keeps_the_dash_for_unknown_membership() {
    let vm = container_row_vm(&summary_with_members(Vec::new()));
    assert_eq!(vm.processes, "—");
    assert!(vm.member_block.is_none());

    let vm = container_row_vm(&summary_with_members(vec![10, 11]));
    assert_eq!(vm.processes, "2");
    assert!(vm.member_block.is_some());
}

/// The page still composes with the inset panel in place.
#[test]
fn containers_page_composes_with_the_inset_member_panel() {
    let app = crate::IcedApp::demo();
    let _view = render(&app);
}
