use gpui::{
    AppContext, Context, IntoElement, Render, ScrollHandle, Styled, TestAppContext,
    VisualTestContext, Window, px, size,
};
use taskforest_core::core::units::UnitPreferences;
use taskmanager_shell::fixture::health::{HealthFixture, health_fixture};

use crate::gpui_app::root::responsive::page_layout_for_viewport;
use taskforest_core::core::SmartSelfTestKind;
use taskmanager_theme::{
    GlassSurfaceRole, HighContrast, LightDark, ResolvedFonts, Skin, Theme, tokens,
};
use taskmanager_ui::theme_binding::{glass_fill, glass_shadow};
use taskmanager_ui_contract::presentation::budget::SystemPageBudget;
use taskmanager_ui_contract::presentation::vocabulary::SystemSurfacePresentation;

use super::{
    SmartSelfTestConfirmationRequest, SystemHealthCallbacks, capture_english_text, inset_surface,
    render_system_health, section_surface,
};

struct FixtureView {
    fixture: HealthFixture,
    theme: Theme,
    layout: SystemPageBudget,
}

impl Render for FixtureView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let callbacks = SystemHealthCallbacks::new(|_request, _window, _cx| {});
        let scroll = ScrollHandle::new();
        render_system_health(crate::gpui_app::system_health_view::SystemHealthViewProps {
            theme: &self.theme,
            scroll: &scroll,
            filesystems: &self.fixture.filesystems,
            sensors: &self.fixture.sensors,
            selected_disk: Some(&self.fixture.selected_disk),
            smart_report: Some(&self.fixture.smart_report),
            layout: self.layout,
            copy: &capture_english_text,
            units: UnitPreferences::default(),
            callbacks: &callbacks,
        })
    }
}

#[test]
fn confirmation_request_is_typed_and_contains_no_execution_plan() {
    let fixture = health_fixture();
    let request = SmartSelfTestConfirmationRequest {
        device_id: fixture.selected_disk.device_id.clone().into(),
        device_generation: fixture.selected_disk.device_generation,
        disk_name: fixture.selected_disk.name.clone(),
        disk_label: fixture.selected_disk.model.clone(),
        kind: SmartSelfTestKind::Extended,
    };
    assert_eq!(request.disk_name, "nvme0n1");
    assert_eq!(request.kind, SmartSelfTestKind::Extended);
}

/// The isolated #12/#13 surface completes layout and paint at both design
/// viewports without invoking a collector, filesystem read, or SMART command.
#[gpui::test]
async fn health_fixture_renders_at_reference_and_compact_sizes(cx: &mut TestAppContext) {
    let window = cx.add_window(|_window, _cx| FixtureView {
        fixture: health_fixture(),
        theme: Theme::dark(),
        layout: SystemPageBudget::from_page_layout(page_layout_for_viewport(size(
            px(1180.0),
            px(780.0),
        ))),
    });
    let mut storage_title_insets = Vec::new();
    let mut sensor_title_insets = Vec::new();
    for (width, height) in [(1180.0, 780.0), (720.0, 480.0)] {
        cx.simulate_window_resize(window.into(), size(px(width), px(height)));
        window
            .update(cx, |view, _window, cx| {
                view.layout = SystemPageBudget::from_page_layout(page_layout_for_viewport(size(
                    px(width),
                    px(height),
                )));
                cx.notify();
            })
            .unwrap();
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        let surface = visual
            .debug_bounds("tm-health-storage-surface")
            .expect("storage surface must remain mounted");
        let title = visual
            .debug_bounds("tm-health-storage-title")
            .expect("storage title must remain mounted");
        assert!(
            surface.origin.x >= px(0.0) && surface.right() <= px(width),
            "{width}x{height}: storage surface leaves the horizontal viewport: {surface:?}"
        );
        storage_title_insets.push(title.origin.y - surface.origin.y);
        let sensor_surface = visual
            .debug_bounds("tm-health-sensor-surface")
            .expect("sensor surface must remain mounted");
        let sensor_title = visual
            .debug_bounds("tm-health-sensor-title")
            .expect("sensor title must remain mounted");
        assert!(
            sensor_surface.origin.x >= px(0.0) && sensor_surface.right() <= px(width),
            "{width}x{height}: sensor surface leaves the horizontal viewport: {sensor_surface:?}"
        );
        sensor_title_insets.push(sensor_title.origin.y - sensor_surface.origin.y);
    }
    assert!(
        storage_title_insets[1] + px(1.0) < storage_title_insets[0],
        "Ultra-Compact must spend less top inset than the multi-column layout: {storage_title_insets:?}"
    );
    assert!(
        sensor_title_insets[1] + px(1.0) < sensor_title_insets[0],
        "Ultra-Compact must spend less top inset in sensors than the multi-column layout: {sensor_title_insets:?}"
    );
}

#[test]
fn health_surfaces_keep_one_page_card_shadow_and_inset_rows_without_shadow() {
    let role = GlassSurfaceRole::PageCard;
    for theme in [
        Theme::dark(),
        Theme::build(
            Skin::Gnome,
            LightDark::Light,
            HighContrast::Off,
            ResolvedFonts::system_for(Skin::Gnome),
        ),
    ] {
        let layout = SystemPageBudget::from_page_layout(page_layout_for_viewport(size(
            px(1280.0),
            px(720.0),
        )));
        let mut section = section_surface(&theme, layout).render();
        let mut row = inset_surface(
            &theme,
            layout,
            tokens::SPACE_6,
            tokens::SPACE_8,
            tokens::control_radius(&theme),
        )
        .render();
        assert_eq!(
            section.style().box_shadow.as_ref(),
            Some(&glass_shadow(&theme, role.tier())),
            "the health section must cast the shared PageCard shadow"
        );
        assert_eq!(
            row.style().box_shadow.as_ref(),
            None,
            "a nested health row must not stack a second shadow"
        );
        let Some(section_fill) = section.style().background.as_ref() else {
            panic!("the health section must paint glass");
        };
        let Some(row_fill) = row.style().background.as_ref() else {
            panic!("the health row must paint glass");
        };
        assert_eq!(*section_fill, glass_fill(&theme, role.tier()));
        assert_eq!(*row_fill, glass_fill(&theme, role.tier()));
    }
}

#[gpui::test]
async fn health_inset_surfaces_stay_inside_their_page_cards(cx: &mut TestAppContext) {
    let window = cx.add_window(|_window, _cx| FixtureView {
        fixture: health_fixture(),
        theme: Theme::dark(),
        layout: SystemPageBudget::from_page_layout(page_layout_for_viewport(size(
            px(1180.0),
            px(780.0),
        ))),
    });
    for (width, height) in [(1280.0, 720.0), (720.0, 480.0)] {
        cx.simulate_window_resize(window.into(), size(px(width), px(height)));
        window
            .update(cx, |view, _window, cx| {
                view.layout = SystemPageBudget::from_page_layout(page_layout_for_viewport(size(
                    px(width),
                    px(height),
                )));
                cx.notify();
            })
            .unwrap();
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        let storage = visual
            .debug_bounds("tm-health-storage-surface")
            .expect("storage surface must remain mounted");
        let filesystem_row = visual
            .debug_bounds("tm-health-filesystem-row")
            .expect("filesystem row must remain mounted");
        let sensor_surface = visual
            .debug_bounds("tm-health-sensor-surface")
            .expect("sensor surface must remain mounted");
        let sensor_group = visual
            .debug_bounds("tm-health-sensor-group:Temperature")
            .expect("temperature sensor group must remain mounted");
        let sensor_row = visual
            .debug_bounds("tm-health-sensor-row:0")
            .expect("first sensor row must remain mounted");
        for (name, bounds, container) in [
            ("filesystem row", filesystem_row, storage),
            ("sensor group", sensor_group, sensor_surface),
            ("sensor row", sensor_row, sensor_surface),
        ] {
            assert!(
                bounds.origin.x >= container.origin.x - px(0.5)
                    && bounds.right() <= container.right() + px(0.5)
                    && bounds.origin.y >= container.origin.y - px(0.5)
                    && bounds.origin.y + bounds.size.height
                        <= container.origin.y + container.size.height + px(0.5),
                "{width}x{height}: {name} leaves its page card: container={container:?}, bounds={bounds:?}"
            );
        }
    }
}

#[gpui::test]
async fn health_single_column_reclaims_space_from_the_section_gap(cx: &mut TestAppContext) {
    let window = cx.add_window(|_window, _cx| FixtureView {
        fixture: health_fixture(),
        theme: Theme::dark(),
        layout: SystemPageBudget::from_page_layout(page_layout_for_viewport(size(
            px(1180.0),
            px(780.0),
        ))),
    });
    for (width, height) in [(1180.0, 780.0), (720.0, 480.0)] {
        cx.simulate_window_resize(window.into(), size(px(width), px(height)));
        window
            .update(cx, |view, _window, cx| {
                view.layout = SystemPageBudget::from_page_layout(page_layout_for_viewport(size(
                    px(width),
                    px(height),
                )));
                cx.notify();
            })
            .unwrap();
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        let storage = visual
            .debug_bounds("tm-health-storage-surface")
            .expect("storage surface must remain mounted");
        let sensors = visual
            .debug_bounds("tm-health-sensor-surface")
            .expect("sensor surface must remain mounted");
        let layout = SystemPageBudget::from_page_layout(page_layout_for_viewport(size(
            px(width),
            px(height),
        )));
        let gap = match layout.surfaces {
            SystemSurfacePresentation::SingleColumn => sensors.origin.y - storage.bottom(),
            SystemSurfacePresentation::MultiColumn => sensors.origin.x - storage.right(),
        };
        let expected = match layout.surfaces {
            SystemSurfacePresentation::SingleColumn => px(tokens::SPACE_7.0),
            SystemSurfacePresentation::MultiColumn => px(tokens::SPACE_10.0),
        };
        assert!(
            (gap - expected).abs() <= px(0.5),
            "{width}x{height}: section gap {gap:?} must follow the typed page fold {expected:?}"
        );
    }
}
