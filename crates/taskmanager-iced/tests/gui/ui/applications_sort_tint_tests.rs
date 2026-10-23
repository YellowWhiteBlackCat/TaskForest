// test-intent: behavior
//! The sorted-column tint: the whole active sort column wears the theme's
//! faint accent wash, and switching the sort moves the wash with it.

use super::*;
use taskmanager_shell::SortCol;
use taskmanager_theme::Theme;

/// The wash the sorted column wears: the theme's own faint accent tint.
fn expected_wash(theme: &Theme) -> iced::Color {
    crate::theme_binding::color(theme.palette().hover)
}

/// The active sort column wears the wash; every other column keeps the row's
/// own surface (no wash at all).
#[test]
fn only_the_active_sort_column_wears_the_wash() {
    let theme = Theme::dark();
    let wash = expected_wash(&theme);

    assert_eq!(
        sort_tint::sort_column_tint(&theme, SortCol::Cpu, SortCol::Cpu),
        Some(wash),
        "the active sort column wears the theme's faint accent wash"
    );
    for column in [
        SortCol::Name,
        SortCol::Pid,
        SortCol::Memory,
        SortCol::DiskRead,
        SortCol::Nice,
    ] {
        assert_eq!(
            sort_tint::sort_column_tint(&theme, SortCol::Cpu, column),
            None,
            "a non-sorted column must keep the row's own surface: {column:?}"
        );
    }
}

/// Switching the sort column moves the wash: the previous column loses it and
/// the new one gains it.
#[test]
fn switching_the_sort_column_moves_the_wash() {
    let theme = Theme::dark();
    let wash = expected_wash(&theme);

    assert_eq!(
        sort_tint::sort_column_tint(&theme, SortCol::Memory, SortCol::Memory),
        Some(wash)
    );
    assert_eq!(
        sort_tint::sort_column_tint(&theme, SortCol::Memory, SortCol::Cpu),
        None,
        "the previously sorted column releases the wash"
    );
    assert_eq!(
        sort_tint::sort_column_tint(&theme, SortCol::DiskRead, SortCol::DiskRead),
        Some(wash),
        "the newly sorted column takes the wash"
    );
}

/// The wash is a translucent surface, not an opaque fill: the row's own
/// selection / zebra surface must still read through it.
#[test]
fn the_wash_is_translucent_over_the_row_surface() {
    let theme = Theme::dark();
    let wash = expected_wash(&theme);
    assert!(
        wash.a > 0.0 && wash.a < 1.0,
        "the sorted-column wash must be translucent, got alpha {}",
        wash.a
    );

    let style = sort_tint::sort_tint_style(&theme, SortCol::Cpu, SortCol::Cpu)
        .expect("the active sort column is tinted");
    let iced::Background::Color(background) = style
        .background
        .expect("the tinted cell paints a background")
    else {
        panic!("the sorted-column wash must be a color background");
    };
    assert_eq!(background, wash);
    assert!(
        sort_tint::sort_tint_style(&theme, SortCol::Cpu, SortCol::Pid).is_none(),
        "an untinted column paints no background at all"
    );
}

/// The projection carries the sort axis it was built for, so the renderer
/// never re-derives the sorted column.
#[test]
fn the_projection_carries_its_sort_axis() {
    use std::collections::HashSet;
    use taskmanager_core::core::process::ProcessItem;
    use taskmanager_core::core::time::LocalTimeRulesObservation;

    let process = ProcessItem::new(42, "worker");
    let flat = vec![&process];
    let projection = crate::ui::process_projection::ProcessProjection::project_with_local_time(
        &flat,
        (SortCol::Memory, SortDir::Desc),
        &HashSet::new(),
        &HashSet::new(),
        &LocalTimeRulesObservation::unsupported(0),
        0,
    );
    assert_eq!(projection.sort_column(), Some(SortCol::Memory));
}
