// test-intent: behavior
//! The Applications table's column geometry: the metric columns keep their
//! x-offsets at every tree depth, and the sticky header sits exactly over
//! the body columns it labels.

use super::*;
use iced::advanced::layout::{Layout, Limits};
use iced::advanced::renderer::Headless;
use iced::advanced::widget::{Id, Operation, Tree};
use iced::{Element, Rectangle, Size};
use std::collections::HashSet;
use taskforest_core::core::process::{ProcessCategory, ProcessItem};
use taskforest_core::core::time::LocalTimeRulesObservation;
use taskmanager_application::process_category_projection::category_expansion_key;
use taskmanager_assets::embedded_fonts;
use taskmanager_shell::{SortCol, SortDir};
use taskmanager_test_support::ProcessItemFixtureBuilder;
use taskmanager_test_support::pin_english;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens::UiSize;

#[derive(Default)]
struct RenderedTextFragments(Vec<(String, Rectangle)>);

impl Operation for RenderedTextFragments {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
        visit(self);
    }

    fn text(&mut self, _id: Option<&Id>, bounds: Rectangle, fragment: &str) {
        self.0.push((fragment.to_owned(), bounds));
    }
}

fn software_renderer() -> iced::Renderer {
    {
        let mut fonts = iced::advanced::graphics::text::font_system()
            .write()
            .expect("font system");
        for font in embedded_fonts() {
            fonts.load_font(font);
        }
    }
    iced::futures::executor::block_on(iced::Renderer::new(
        crate::theme_binding::BUNDLED_UI_FONT,
        iced::Pixels(16.0),
        Some("tiny-skia"),
    ))
    .expect("software renderer")
}

fn rendered_text(
    element: &mut Element<'_, Message, iced::Theme, iced::Renderer>,
    renderer: &iced::Renderer,
) -> Vec<(String, Rectangle)> {
    let mut tree = Tree::new(&*element);
    let node = element.as_widget_mut().layout(
        &mut tree,
        renderer,
        &Limits::new(Size::ZERO, Size::new(2_400.0, 480.0)),
    );
    let mut fragments = RenderedTextFragments::default();
    element
        .as_widget_mut()
        .operate(&mut tree, Layout::new(&node), renderer, &mut fragments);
    fragments.0
}

fn right_edge(fragments: &[(String, Rectangle)], text: &str) -> Option<f32> {
    fragments
        .iter()
        .find(|(fragment, bounds)| fragment == text && bounds.width > 0.0)
        .map(|(_, bounds)| bounds.x + bounds.width)
}

/// One process per tree level, each the child of the previous one: six
/// levels of nesting, far beyond the five the acceptance names.
fn process_tree() -> Vec<ProcessItem> {
    (0..6_u32)
        .map(|level| {
            let mut process = ProcessItemFixtureBuilder::new()
                .pid(90_000 + level)
                .name(format!("capture-worker-{level}"))
                .status("S".to_owned())
                .build();
            process.parent_pid = (level > 0).then(|| 90_000 + level - 1);
            process
        })
        .collect()
}

/// The projected rows of the expanded tree, with the depth of every process
/// row.
fn projected_tree() -> crate::ui::process_projection::ProcessProjection {
    let processes = process_tree();
    let flat: Vec<&ProcessItem> = processes.iter().collect();
    let mut expanded = HashSet::new();
    expanded.insert(category_expansion_key(ProcessCategory::Uncategorized));
    crate::ui::process_projection::ProcessProjection::project_with_local_time(
        &flat,
        (SortCol::Name, SortDir::Asc),
        &expanded,
        &HashSet::new(),
        &LocalTimeRulesObservation::unsupported(0),
        0,
    )
}

/// The row context the Applications page builds for one frame.
fn row_context(theme: &Theme) -> RowRender {
    RowRender {
        theme: *theme,
        query: String::new(),
        search_active: false,
        swap_visible: true,
        compact: false,
        ui_size: UiSize::Standard,
        selected_identities: std::rc::Rc::new(HashSet::new()),
        selected_row: None,
        gray_zero: false,
        hidden_columns: std::rc::Rc::new(HashSet::new()),
        column_widths: std::rc::Rc::new(crate::app::ColumnWidthOverrides::default()),
        open_menu_identity: None,
    }
}

/// The metric columns keep the same x-offsets at every tree depth: the depth
/// indent lives inside the identity cell and never pushes the metric
/// columns.
#[test]
fn the_metric_columns_keep_their_offsets_at_every_tree_depth() {
    pin_english();
    let theme = Theme::dark();
    let renderer = software_renderer();
    let projection = projected_tree();
    let ctx = row_context(&theme);

    // The tree must really be deep: the assertion is about a five-level
    // expansion, not a flat list.
    let depths: Vec<usize> = projection
        .rows()
        .iter()
        .filter_map(|row| match row {
            crate::ui::process_projection::ProjectedRow::Tree { depth, .. } => Some(*depth),
            _ => None,
        })
        .collect();
    assert!(
        depths.iter().copied().max().unwrap_or(0) >= 5,
        "the fixture must expand at least five levels, got {depths:?}"
    );

    let mut pid_offsets: Vec<(usize, f32)> = Vec::new();
    for (index, row) in projection.rows().iter().enumerate() {
        let crate::ui::process_projection::ProjectedRow::Tree { pid, depth, .. } = row else {
            continue;
        };
        let mut element = applications_table_rows_range(&ctx, &projection, index, index + 1)
            .into_iter()
            .next()
            .expect("the projected row renders one element");
        let fragments = rendered_text(&mut element, &renderer);
        let pid_text = pid.to_string();
        let offset = right_edge(&fragments, &pid_text).unwrap_or_else(|| {
            panic!("the pid cell must render for depth {depth}: {fragments:?}");
        });
        pid_offsets.push((*depth, offset));
    }

    assert!(
        pid_offsets.len() >= 5,
        "the fixture must render at least five tree rows: {pid_offsets:?}"
    );
    let first = pid_offsets[0].1;
    for (depth, offset) in &pid_offsets {
        assert!(
            (offset - first).abs() <= 0.5,
            "the pid column moved at depth {depth}: {offset} != {first} ({pid_offsets:?})"
        );
    }
}

/// The sticky header sits exactly over the body columns: a header caption and
/// the body cell under it share one column boundary (the header caption
/// carries the button's own inset).
#[test]
fn the_sticky_header_sits_over_the_body_columns() {
    pin_english();
    let theme = Theme::dark();
    let renderer = software_renderer();
    let projection = projected_tree();
    let ctx = row_context(&theme);

    let columns = visible_apps_columns(true, &HashSet::new());
    let extent = apps_table_width_with(true, &HashSet::new(), &ColumnWidthOverrides::default());
    let mut header = applications_header_row(
        &theme,
        &columns,
        (SortCol::Name, SortDir::Asc),
        false,
        extent,
    );
    let header_fragments = rendered_text(&mut header, &renderer);

    // The first process row of the tree.
    let row_index = projection
        .rows()
        .iter()
        .position(|row| {
            matches!(
                row,
                crate::ui::process_projection::ProjectedRow::Tree { .. }
            )
        })
        .expect("the fixture has process rows");
    let mut body = applications_table_rows_range(&ctx, &projection, row_index, row_index + 1)
        .into_iter()
        .next()
        .expect("the projected row renders one element");
    let body_fragments = rendered_text(&mut body, &renderer);

    // The PID column: a numeric column, so both the header caption and the
    // body value are right-aligned inside their cell.
    let header_right = right_edge(&header_fragments, "PID")
        .unwrap_or_else(|| panic!("the pid header must render: {header_fragments:?}"));
    let body_right = right_edge(&body_fragments, "90000")
        .unwrap_or_else(|| panic!("the pid cell must render: {body_fragments:?}"));
    assert!(
        (body_right - header_right - HEADER_CELL_INSET_X).abs() <= 0.5,
        "the header caption and the body cell must share one column boundary: \
         header {header_right} vs body {body_right}"
    );
}
