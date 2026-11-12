use super::card;
use super::facets::gpu_layout;
use gpui::{Context, IntoElement, Render, Styled, TestAppContext, Window, px};
use taskmanager_shell::fixture::process_insights::process_insights_snapshot;
use taskmanager_theme::{GlassSurfaceRole, Theme};
use taskmanager_ui::init;
use taskmanager_ui::theme_binding::glass_fill;

#[test]
fn insight_cards_use_the_dialog_nested_surface_without_a_second_shadow() {
    let theme = Theme::dark();
    let mut panel = card(&theme, "Network throughput", 480.0);
    let tier = GlassSurfaceRole::InlinePlot.tier();

    assert_eq!(
        panel.style().background.as_ref(),
        Some(&glass_fill(&theme, tier)),
        "insight panels must spend the shared inset glass material"
    );
    assert!(
        panel
            .style()
            .box_shadow
            .as_ref()
            .is_none_or(|shadows| shadows.is_empty()),
        "a panel inside the properties dialog must not stack a second shadow"
    );
}

fn fixture_labels() -> &'static super::ProcessInsightsLabels {
    Box::leak(Box::new(super::ProcessInsightsLabels::capture_fixture()))
}

struct GpuSlotsView {
    theme: Theme,
    width: f32,
}

impl Render for GpuSlotsView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let fixture = process_insights_snapshot();
        gpu_layout(
            &self.theme,
            &fixture.gpu,
            fixture_labels(),
            self.width,
            taskforest_core::core::units::UnitPreferences::default(),
        )
        .w(px(self.width))
    }
}

#[gpui::test]
async fn gpu_insight_slots_admit_two_groups_only_at_the_two_column_budget(cx: &mut TestAppContext) {
    use gpui::{AppContext, VisualTestContext, px};

    let theme = Theme::dark();
    cx.update(init);

    let draw_at = |cx: &mut TestAppContext, width: f32| {
        let window = cx.add_window(|_window, _cx| GpuSlotsView { theme, width });
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        window
    };

    let wide_window = draw_at(cx, 650.0);
    let mut wide = VisualTestContext::from_window(wide_window.into(), cx);
    let summary = wide
        .debug_bounds("tm-insights-gpu-summary")
        .expect("wide summary slot");
    let engines = wide
        .debug_bounds("tm-insights-gpu-engines")
        .expect("wide engines slot");
    assert!(summary.origin.x < engines.origin.x);
    assert_eq!(summary.origin.y, engines.origin.y);
    assert_eq!(summary.size.width, engines.size.width);
    let grid = wide
        .debug_bounds("tm-insights-gpu-grid")
        .expect("wide adaptive grid");
    assert!(grid.size.width <= px(650.0));

    let compact_window = draw_at(cx, 649.0);
    let mut compact = VisualTestContext::from_window(compact_window.into(), cx);
    let summary = compact
        .debug_bounds("tm-insights-gpu-summary")
        .expect("compact summary slot");
    let engines = compact
        .debug_bounds("tm-insights-gpu-engines")
        .expect("compact engines slot");
    assert!(engines.origin.y >= summary.bottom());
    assert!(engines.size.width <= summary.size.width);
}
