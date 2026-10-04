//! test-intent: behavior
//! Review geometry over measured embedded fonts at small, tall and wide sizes.

use super::*;
use crate::pages::history::HistoryProjectionResource;
use crate::pages::history::control::{
    PerformanceHistoryProjectionResource, PerformancePresentation,
};
use crate::pages::performance::replay::{PerformanceReplayBody, PerformanceReplayRoot};
use crate::widgets::history_controls::{HistoryActionGroup, HistoryControl};
use bevy::camera::{Camera, Camera2d, ComputedCameraValues, RenderTargetInfo, Viewport};
use bevy::ecs::query::Has;
use bevy::image::TextureAtlasLayout;
use bevy::math::UVec2;
use bevy::picking::DefaultPickingPlugins;
use bevy::text::TextPlugin;
use bevy::transform::TransformPlugin;
use bevy::ui::widget::Text;
use bevy::ui::{ComputedNode, UiGlobalTransform, UiPlugin};
use bevy::ui_widgets::ScrollArea;
use bevy::window::{ExitCondition, PrimaryWindow, Window, WindowPlugin};
use std::sync::Arc;
use taskmanager_application::i18n::t;
use taskmanager_application::{
    ApplicationHistoryCapability, HistoryReplayCompletion, HistoryReplayCompletionOutcome,
    HistoryReplayController, HistoryReplayRow,
};
use taskmanager_core::core::history::{
    ApplicationHistoryIdentity, HistoryMetric, HistorySeriesKey,
};
use taskmanager_core::core::identity::DeviceId;

#[test]
fn review_title_actions_and_scroll_owner_fit_measured_font_bounds_and_keep_graphs_stable() {
    for page in [Page::Performance, Page::AppHistory] {
        for (width, height) in [(480, 360), (720, 480), (1280, 720), (1600, 360), (480, 960)] {
            let cache: &'static RuntimeCache = Box::leak(Box::new(RuntimeCache::new()));
            let runtime = cache
                .get_or_init_with_config(|| Ok((fake_client(), None)))
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
            let samples: Arc<[f32]> = Arc::from([4.0, 20.0, f32::NAN, 17.0, 83.0]);
            let times: Arc<[u64]> = Arc::from([1_000, 2_000, 3_000, 4_000, 5_000]);
            let application = ApplicationHistoryIdentity::verified_launcher(
                "io.example.VeryLongApplicationIdentityThatMustStayWithinItsOwnRow",
            )
            .expect("identity");
            let keys = [
                HistorySeriesKey::for_device(
                    HistoryMetric::GpuUsagePct,
                    DeviceId::new(
                        "very-long-device-identity-that-must-wrap-inside-its-allocated-viewport",
                    ),
                ),
                HistorySeriesKey::for_application(
                    HistoryMetric::ApplicationCpuUsagePct,
                    application,
                ),
            ];
            let rows = keys
                .into_iter()
                .map(|key| HistoryReplayRow {
                    key,
                    samples: Arc::clone(&samples),
                    sample_times_ms: Arc::clone(&times),
                    peak_value: Some(83.0),
                    peak_measured_at_ms: Some(5_000),
                    observed: 4,
                    gaps: 1,
                    clock_jumps: 0,
                })
                .collect::<Vec<_>>();
            let mut controller = HistoryReplayController::default();
            let request = controller.open().expect("query");
            controller.complete(HistoryReplayCompletion {
                request,
                loaded_at_ms: 5_000,
                outcome: HistoryReplayCompletionOutcome::Loaded(rows.into()),
            });
            app.world_mut().resource_mut::<Route>().page = page;
            app.world_mut()
                .resource_mut::<HistoryProjectionResource>()
                .0 =
                controller.application_history_projection(ApplicationHistoryCapability::Available);
            app.world_mut()
                .resource_mut::<PerformanceHistoryProjectionResource>()
                .0 =
                controller.performance_history_projection(ApplicationHistoryCapability::Available);
            *app.world_mut().resource_mut::<PerformancePresentation>() =
                PerformancePresentation::Replay;
            for _ in 0..6 {
                app.update();
            }
            let world = app.world_mut();
            let mut query = world.query::<(
                &ComputedNode,
                &UiGlobalTransform,
                Has<PerformanceReplayRoot>,
                Has<PerformanceReplayBody>,
                Has<HistoryActionGroup>,
                Has<HistoryControl>,
                Has<ScrollArea>,
            )>();
            let mut groups = 0;
            let mut scrolls = 0;
            for (node, transform, root, body, group, control, scroll) in query.iter(world) {
                if node.size().x == 0.0 || !(root || body || group || control || scroll) {
                    continue;
                }
                if group {
                    groups += 1;
                }
                if scroll {
                    scrolls += 1;
                }
                let half = node.size() / 2.0;
                assert!(
                    node.size().y > 0.0,
                    "mandatory review slot retains height at {width}x{height}"
                );
                assert!(
                    transform.translation.x - half.x >= -0.5
                        && transform.translation.x + half.x <= width as f32 + 0.5,
                    "right/left bounds at {width}x{height}: {:?} {:?}",
                    transform.translation,
                    node.size()
                );
                assert!(
                    transform.translation.y - half.y >= -0.5
                        && transform.translation.y + half.y <= height as f32 + 0.5,
                    "top/bottom bounds at {width}x{height}: {:?} {:?}",
                    transform.translation,
                    node.size()
                );
            }
            assert_eq!(groups, 1, "one complete visible action group");
            assert!(scrolls >= 1, "the review owns a bounded scroll viewport");
            let mut texts = world.query::<(&Text, &ComputedNode)>();
            assert!(
                texts.iter(world).any(|(text, node)| text.0
                    == t(if page == Page::Performance {
                        "perf.replay.title"
                    } else {
                        "history.application.title"
                    })
                    && node.size().y > 0.0),
                "the assertion uses measured fonts, not zero-sized text placeholders"
            );
            let entities = world.query::<Entity>().iter(world).collect::<Vec<_>>();
            app.update();
            let after = app
                .world_mut()
                .query::<Entity>()
                .iter(app.world())
                .collect::<Vec<_>>();
            assert_eq!(
                after, entities,
                "an idle frame preserves the mounted review graph hierarchy"
            );
        }
    }
}
