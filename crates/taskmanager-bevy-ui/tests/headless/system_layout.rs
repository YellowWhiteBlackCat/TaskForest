//! test-intent: behavior
//! Normal authorization, correlated results and measured System viewport bounds.

use super::*;
use crate::app::NavTarget;
use crate::pages::settings::privilege_center::PrivilegeControl;
use crate::pages::system::{SystemActions, SystemBody, SystemStatusLine};
use bevy::camera::{Camera, Camera2d, ComputedCameraValues, RenderTargetInfo, Viewport};
use bevy::ecs::query::{Has, With};
use bevy::image::TextureAtlasLayout;
use bevy::math::UVec2;
use bevy::picking::DefaultPickingPlugins;
use bevy::text::TextPlugin;
use bevy::transform::TransformPlugin;
use bevy::ui::widget::Text;
use bevy::ui::{ComputedNode, UiGlobalTransform, UiPlugin};
use bevy::ui_widgets::{Activate, ScrollArea};
use bevy::window::{ExitCondition, PrimaryWindow, Window, WindowPlugin};
use taskmanager_application::{PlatformClient, SmbiosMemoryState, i18n::t};
use taskmanager_shell::demo_app;
use taskmanager_shell::fixture::smbios_memory::memory_inventory_snapshot;
use taskmanager_shell::presentation::privilege_center::PrivilegeAction;
use taskmanager_test_support::{pin_english, smbios_memory};

#[test]
fn memory_permission_entry_and_system_actions_fit_real_font_viewport_bounds() {
    pin_english();
    for (width, height) in [(480, 360), (720, 480), (1280, 720), (1600, 360), (480, 960)] {
        let value = memory_inventory_snapshot();
        let (client, recorder) = smbios_memory::platform(value.clone());
        let mut app = system_layout_app(width, height, client);
        app.world_mut().non_send_mut::<FrontendTrack>().shell = demo_app();
        app.world_mut().resource_mut::<Route>().page = Page::Settings;
        for _ in 0..6 {
            app.update();
        }
        let control = app
            .world_mut()
            .query::<(Entity, &PrivilegeControl)>()
            .iter(app.world())
            .find(|(_, control)| control.0 == Some(PrivilegeAction::SmbiosMemory))
            .map(|(entity, _)| entity)
            .expect("normal memory authorization offer");
        app.world_mut().trigger(Activate { entity: control });
        app.world_mut().trigger(Activate { entity: control });
        for _ in 0..8 {
            app.update();
        }
        let SmbiosMemoryState::Ready(ready) = app
            .world()
            .non_send::<FrontendTrack>()
            .shell
            .smbios_memory_state()
        else {
            panic!("normal request and drain must publish memory inventory");
        };
        assert_eq!(ready.snapshot, value);
        assert_eq!(
            recorder.submissions().expect("requests").len(),
            1,
            "two activations in one frame authorize only once"
        );
        let system_tab = app
            .world_mut()
            .query::<(Entity, &NavTarget)>()
            .iter(app.world())
            .find(|(_, target)| target.0 == Page::System)
            .map(|(entity, _)| entity)
            .expect("System navigation tab");
        app.world_mut().trigger(Activate { entity: system_tab });
        for _ in 0..6 {
            app.update();
        }
        let world = app.world_mut();
        let mut query = world.query::<(
            &ComputedNode,
            &UiGlobalTransform,
            Has<SystemActions>,
            Has<SystemBody>,
            Has<SystemStatusLine>,
            Has<ScrollArea>,
        )>();
        let mut slots = 0;
        for (node, transform, actions, body, status, scroll) in query.iter(world) {
            if !(actions || body || status) {
                continue;
            }
            slots += 1;
            let half = node.size() / 2.0;
            assert!(node.size().y > 0.0);
            assert!(
                transform.translation.x - half.x >= -0.5
                    && transform.translation.x + half.x <= width as f32 + 0.5,
                "System slot at {width}x{height}, actions={actions}, body={body}, status={status}, center={:?}, size={:?}",
                transform.translation,
                node.size()
            );
            assert!(
                transform.translation.y - half.y >= -0.5
                    && transform.translation.y + half.y <= height as f32 + 0.5
            );
            if body {
                assert!(scroll, "the fact body owns native scrolling");
            }
        }
        assert_eq!(slots, 3);
        let mut text = world.query::<(&Text, &ComputedNode)>();
        assert!(
            text.iter(world)
                .any(|(text, node)| text.0 == "ChannelB-DIMM0" && node.size().y > 0.0),
            "the accepted inventory is rendered with measured product fonts"
        );
    }
}

fn system_layout_app(width: u32, height: u32, client: PlatformClient) -> App {
    let cache: &'static RuntimeCache = Box::leak(Box::new(RuntimeCache::new()));
    let runtime = cache
        .get_or_init_with_config(|| Ok((client, None)))
        .expect("quiet runtime");
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(HeadlessFrontendPlugins)
        .add_plugins(FrontendWindowPlugin {
            runtime,
            palette: ui_palette(&Theme::dark()),
        });
    app.init_resource::<Assets<Font>>();
    app.add_plugins((
        WindowPlugin {
            primary_window: None,
            exit_condition: ExitCondition::DontExit,
            ..Default::default()
        },
        DefaultPickingPlugins,
        TransformPlugin,
        TextPlugin,
        UiPlugin,
    ));
    app.init_resource::<Assets<TextureAtlasLayout>>();
    let size = UVec2::new(width, height);
    app.world_mut().spawn((
        Window {
            resolution: (width, height).into(),
            ..Default::default()
        },
        PrimaryWindow,
    ));
    app.world_mut().spawn((
        Camera2d,
        Camera {
            computed: ComputedCameraValues {
                target_info: Some(RenderTargetInfo {
                    physical_size: size,
                    scale_factor: 1.0,
                }),
                ..Default::default()
            },
            viewport: Some(Viewport {
                physical_size: size,
                ..Default::default()
            }),
            ..Default::default()
        },
    ));

    app
}

#[test]
fn normal_dashboard_windows_and_paging_preserve_measured_whole_curve_groups() {
    use crate::pages::system::dashboard::{
        DashboardControl, DashboardControlButton, SystemDashboardCurve, SystemDashboardState,
        SystemDashboardToolbar,
    };
    use taskmanager_application::system_timeline::{
        SystemHistoryWindow, SystemPageSection, TimelineMetric,
    };
    use taskmanager_shell::fixture::dashboard_history::seed_shell_system_dashboard_history;
    pin_english();
    for (width, height) in [(480, 360), (720, 480), (1280, 720), (1600, 360), (480, 960)] {
        let (client, _) = smbios_memory::platform(memory_inventory_snapshot());
        let mut app = system_layout_app(width, height, client);
        let mut shell = demo_app();
        assert!(seed_shell_system_dashboard_history(&mut shell, 7_200_000));
        app.world_mut().non_send_mut::<FrontendTrack>().shell = shell;
        app.world_mut().resource_mut::<Route>().page = Page::System;
        for _ in 0..6 {
            app.update();
        }
        activate_dashboard_control(
            &mut app,
            DashboardControl::Section(SystemPageSection::Dashboard),
        );
        for window in SystemHistoryWindow::ALL {
            activate_dashboard_control(&mut app, DashboardControl::Window(window));
            assert_eq!(
                app.world().resource::<SystemDashboardState>().window,
                window
            );
            let series = app
                .world()
                .non_send::<FrontendTrack>()
                .shell
                .system_timeline_series(window);
            for metric in TimelineMetric::ALL {
                assert_eq!(series.coverage_ms(metric), window.minutes() * 60_000);
            }
        }
        let world = app.world_mut();
        let mut bounds = world.query::<(
            &ComputedNode,
            &UiGlobalTransform,
            Has<SystemBody>,
            Has<SystemDashboardToolbar>,
        )>();
        let body = bounds
            .iter(world)
            .find(|(_, _, body, _)| *body)
            .map(|(node, transform, _, _)| (node.size(), transform.translation))
            .expect("System viewport");
        assert!(body.0.y > 0.0);
        for (node, transform, is_body, toolbar) in bounds.iter(world) {
            if !(is_body || toolbar) {
                continue;
            }
            let half = node.size() / 2.0;
            assert!(
                transform.translation.x - half.x >= -0.5
                    && transform.translation.x + half.x <= width as f32 + 0.5
            );
            assert!(
                transform.translation.y - half.y >= -0.5
                    && transform.translation.y + half.y <= height as f32 + 0.5
            );
        }
        let mut curves =
            world.query::<(&SystemDashboardCurve, &ComputedNode, &UiGlobalTransform)>();
        let initial = curves.iter(world).count();
        if initial == 0 {
            assert!(
                world
                    .query::<&Text>()
                    .iter(world)
                    .any(|text| text.0 == t("dashboard.resize"))
            );
        } else {
            let mut visited = Vec::new();
            for _ in 0..4 {
                let world = app.world_mut();
                for (curve, node, transform) in world
                    .query::<(&SystemDashboardCurve, &ComputedNode, &UiGlobalTransform)>()
                    .iter(world)
                {
                    visited.push(curve.metric);
                    let half = node.size() / 2.0;
                    assert!(node.size().y > 0.0 && node.size().x > 0.0);
                    assert!(
                        transform.translation.y + half.y <= body.1.y + body.0.y / 2.0 + 0.5,
                        "last curve must fit its body at {width}x{height}"
                    );
                    assert!(transform.translation.x + half.x <= body.1.x + body.0.x / 2.0 + 0.5);
                }
                activate_dashboard_control(&mut app, DashboardControl::Next);
            }
            for metric in TimelineMetric::ALL {
                assert!(
                    visited.contains(&metric),
                    "normal paging reaches {metric:?} at {width}x{height}"
                );
            }
        }
        activate_dashboard_control(
            &mut app,
            DashboardControl::Section(SystemPageSection::Hardware),
        );
        assert_eq!(
            app.world().resource::<SystemDashboardState>().section,
            SystemPageSection::Hardware
        );
    }
    fn activate_dashboard_control(app: &mut App, target: DashboardControl) {
        let entity = app
            .world_mut()
            .query::<(Entity, &DashboardControlButton)>()
            .iter(app.world())
            .find(|(_, button)| match (button.0, target) {
                (DashboardControl::Section(left), DashboardControl::Section(right)) => {
                    left == right
                }
                (DashboardControl::Window(left), DashboardControl::Window(right)) => left == right,
                (DashboardControl::Next, DashboardControl::Next) => true,
                _ => false,
            })
            .map(|(entity, _)| entity)
            .expect("normal dashboard control");
        app.world_mut().trigger(Activate { entity });
        for _ in 0..6 {
            app.update();
        }
    }
}

#[test]
fn normal_properties_tabs_preserve_frozen_identity_and_measured_inspection_bounds() {
    use crate::demo_fixture::seed_capture_confirmation_scenario;
    use crate::input::ShellInteractionApplied;
    use crate::pages::processes::properties_modal::{
        ProcessPropertiesBody, ProcessPropertiesFacet, ProcessPropertiesFooter,
        ProcessPropertiesPanel, ProcessPropertiesPresentation, ProcessPropertiesScrollHint,
        ProcessPropertiesSection, ProcessPropertiesTab,
    };
    use taskmanager_application::ProcessInsightFacet;
    pin_english();
    for (width, height) in [
        (480, 360),
        (720, 480),
        (1280, 720),
        (1600, 360),
        (480, 960),
        (1920, 1080),
    ] {
        let (client, _) = smbios_memory::platform(memory_inventory_snapshot());
        let mut app = system_layout_app(width, height, client);
        let mut shell = demo_app();
        seed_capture_confirmation_scenario(&mut shell, "process-memory-pss-swap");
        let frozen = shell
            .process_properties_target()
            .expect("normal frozen entry")
            .clone();
        app.world_mut().non_send_mut::<FrontendTrack>().shell = shell;
        app.world_mut().resource_mut::<Route>().page = Page::Processes;
        app.world_mut().trigger(ShellInteractionApplied);
        for _ in 0..8 {
            app.update();
        }
        for section in ProcessPropertiesSection::ALL {
            let entity = app
                .world_mut()
                .query::<(Entity, &ProcessPropertiesTab)>()
                .iter(app.world())
                .find(|(_, tab)| tab.0 == section)
                .map(|(entity, _)| entity)
                .expect("normal tab");
            app.world_mut().trigger(Activate { entity });
            for _ in 0..6 {
                app.update();
            }
            assert_eq!(
                app.world()
                    .resource::<ProcessPropertiesPresentation>()
                    .section,
                section
            );
            assert_eq!(
                app.world()
                    .non_send::<FrontendTrack>()
                    .shell
                    .process_properties_target(),
                Some(&frozen)
            );
            let world = app.world_mut();
            let mut bounds = world.query::<(
                &ComputedNode,
                &UiGlobalTransform,
                Has<ProcessPropertiesPanel>,
                Has<ProcessPropertiesBody>,
                Has<ProcessPropertiesFooter>,
            )>();
            let panel = bounds
                .iter(world)
                .find(|(_, _, panel, _, _)| *panel)
                .map(|(node, transform, _, _, _)| (node.size(), transform.translation))
                .expect("panel");
            let body = bounds
                .iter(world)
                .find(|(_, _, _, body, _)| *body)
                .map(|(node, transform, _, _, _)| (node.size(), transform.translation))
                .expect("body");
            let footer = bounds
                .iter(world)
                .find(|(_, _, _, _, footer)| *footer)
                .map(|(node, transform, _, _, _)| (node.size(), transform.translation))
                .expect("footer");
            assert!(
                body.0.x > 0.0 && body.0.y > 0.0,
                "readable body at {width}x{height}"
            );
            assert!(
                panel.1.x - panel.0.x / 2.0 >= -0.5
                    && panel.1.x + panel.0.x / 2.0 <= width as f32 + 0.5
            );
            assert!(
                panel.1.y - panel.0.y / 2.0 >= -0.5
                    && panel.1.y + panel.0.y / 2.0 <= height as f32 + 0.5
            );
            assert!(
                body.1.y + body.0.y / 2.0 <= footer.1.y - footer.0.y / 2.0 + 0.5,
                "fixed footer after body"
            );
            assert!(
                footer.1.y + footer.0.y / 2.0 <= panel.1.y + panel.0.y / 2.0 - 4.0,
                "bottom inset"
            );
            let (hint_node, hint_text) = app
                .world_mut()
                .query_filtered::<(&ComputedNode, &Text), With<ProcessPropertiesScrollHint>>()
                .single(app.world())
                .expect("visible scroll hint");
            assert!(hint_node.size().x > 0.0 && hint_node.size().y >= 14.0);
            assert_eq!(hint_text.0, t("proc_insights.scroll_hint"));
            if section == ProcessPropertiesSection::Insights {
                for facet in [
                    ProcessInsightFacet::Network,
                    ProcessInsightFacet::Gpu,
                    ProcessInsightFacet::Resources,
                    ProcessInsightFacet::Isolation,
                ] {
                    let entity = app
                        .world_mut()
                        .query::<(Entity, &ProcessPropertiesFacet)>()
                        .iter(app.world())
                        .find(|(_, button)| button.0 == facet)
                        .map(|(entity, _)| entity)
                        .expect("normal facet");
                    app.world_mut().trigger(Activate { entity });
                    for _ in 0..4 {
                        app.update();
                    }
                    assert_eq!(
                        app.world()
                            .resource::<ProcessPropertiesPresentation>()
                            .facet,
                        facet
                    );
                }
            }
        }
    }
}

#[test]
fn alert_rule_controls_and_scroll_body_fit_measured_product_viewports() {
    use crate::pages::alerts::editor::RuleButton;
    pin_english();
    for (width, height) in [
        (480, 360),
        (720, 480),
        (1280, 720),
        (1600, 360),
        (480, 960),
        (980, 560),
    ] {
        let (client, _) = smbios_memory::platform(memory_inventory_snapshot());
        let mut app = system_layout_app(width, height, client);
        app.world_mut().non_send_mut::<FrontendTrack>().shell = demo_app();
        app.world_mut().resource_mut::<Route>().page = Page::Alerts;
        for _ in 0..6 {
            app.update();
        }
        let world = app.world_mut();
        let mut buttons = world.query::<(&RuleButton, &ComputedNode, &UiGlobalTransform)>();
        let controls: Vec<_> = buttons.iter(world).collect();
        assert!(
            controls.len() >= 4 + 5 * 10,
            "normal rule editing controls must be mounted"
        );
        for (_button, node, transform) in controls {
            let half = node.size() * 0.5;
            assert!(node.size().x > 0.0 && node.size().y > 0.0);
            assert!(
                transform.translation.x - half.x >= -0.5
                    && transform.translation.x + half.x <= width as f32 + 0.5,
                "right edge protected at {width} × {height}"
            );
        }
        let mut bodies =
            world.query_filtered::<(&ComputedNode, &UiGlobalTransform), With<ScrollArea>>();
        let (node, transform) = bodies.single(world).expect("one bounded rule body");
        let half = node.size() * 0.5;
        assert!(node.size().y > 0.0);
        assert!(
            transform.translation.y - half.y >= -0.5
                && transform.translation.y + half.y <= height as f32 + 0.5,
            "body remains inside the bottom edge"
        );
    }
}
