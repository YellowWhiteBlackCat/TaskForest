//! test-intent: behavior
//! Normal authorization, correlated results and measured System viewport bounds.

use super::*;
use crate::app::NavTarget;
use crate::pages::settings::privilege_center::PrivilegeControl;
use crate::pages::system::{SystemActions, SystemBody, SystemStatusLine};
use bevy::camera::{Camera, Camera2d, ComputedCameraValues, RenderTargetInfo, Viewport};
use bevy::ecs::query::Has;
use bevy::image::TextureAtlasLayout;
use bevy::math::UVec2;
use bevy::picking::DefaultPickingPlugins;
use bevy::text::TextPlugin;
use bevy::transform::TransformPlugin;
use bevy::ui::widget::Text;
use bevy::ui::{ComputedNode, UiGlobalTransform, UiPlugin};
use bevy::ui_widgets::{Activate, ScrollArea};
use bevy::window::{ExitCondition, PrimaryWindow, Window, WindowPlugin};
use taskmanager_application::SmbiosMemoryState;
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
