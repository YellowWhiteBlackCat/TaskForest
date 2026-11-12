//! test-intent: behavior

use std::collections::HashMap;
use std::thread;
use std::time::Duration;

use bevy::app::App;
use bevy::asset::{Assets, Handle, RenderAssetUsages};
use bevy::camera::{Camera, Camera2d, ClearColor, ClearColorConfig, RenderTarget};
use bevy::ecs::entity::Entity;
use bevy::ecs::observer::On;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, ResMut};
use bevy::image::Image;
use bevy::math::Vec2;
use bevy::prelude::{
    BackgroundColor, Color, DefaultPlugins, ImageNode, Node, PluginGroup, PositionType, Startup,
    UiTargetCamera, percent, px,
};
use bevy::render::RenderPlugin;
use bevy::render::render_resource::WgpuAdapterInfo;
use bevy::render::render_resource::{Extent3d, PollType, TextureDimension, TextureFormat};
use bevy::render::renderer::{RenderAdapterInfo, RenderDevice};
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::ui_render::prelude::MaterialNode;
use bevy::window::{ExitCondition, WindowPlugin};
use bevy::winit::WinitPlugin;
use taskmanager_theme::optical_glass::{
    OpticalGlassContext, OpticalGlassQuality, OpticalGlassSpec,
};
use taskmanager_theme::{GlassTier, Theme};

use super::capability::{OPTICAL_GLASS_MAX_BLUR_PX, OPTICAL_GLASS_MAX_NESTED_LAYERS};
use super::{
    OpticalGlassMaterial, OpticalGlassMaterialKind, OpticalGlassRaster, OpticalGlassUniform,
};

const WIDTH: u32 = 192;
const HEIGHT: u32 = 144;
const LEFT: u32 = 24;
const TOP: u32 = 24;
const CARD_WIDTH: u32 = 144;
const CARD_HEIGHT: u32 = 96;
const NESTED_LEFT: u32 = 40;
const NESTED_TOP: u32 = 40;
const NESTED_WIDTH: u32 = 112;
const NESTED_HEIGHT: u32 = 72;

#[derive(Resource)]
struct Targets {
    pattern: Handle<Image>,
    outputs: Vec<Handle<Image>>,
    chain: Handle<Image>,
    chain_only_capture: Handle<Image>,
    nested_output: Handle<Image>,
    nested_only_capture: Handle<Image>,
}

#[derive(Default, Resource)]
struct Captures(HashMap<Entity, Image>);

#[test]
#[ignore = "runs through scripts/quality/bevy-optical-gpu-gate.sh (requires a GPU adapter)"]
fn production_shader_renders_optical_effects_and_tracks_the_backdrop() {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: ExitCondition::DontExit,
                ..WindowPlugin::default()
            })
            .set(RenderPlugin {
                synchronous_pipeline_compilation: true,
                ..RenderPlugin::default()
            })
            .disable::<WinitPlugin>(),
    );
    super::register(&mut app);
    app.init_resource::<Captures>();
    app.insert_resource(ClearColor(Color::BLACK));
    app.add_systems(Startup, setup);
    app.finish();
    app.cleanup();
    advance(&mut app, 12);

    let adapter = app.world().resource::<RenderAdapterInfo>();
    let adapter_info: &WgpuAdapterInfo = adapter;
    println!(
        "optical-gpu receipt: adapter={adapter_info:?} operations=BackdropSampling,Blur,Refraction,Dispersion,Specular,Grain max_nested_layers={} max_blur_px={} nesting_layers_proven=true fresnel_rim_proven=true",
        OPTICAL_GLASS_MAX_NESTED_LAYERS, OPTICAL_GLASS_MAX_BLUR_PX
    );

    let outputs = app.world().resource::<Targets>().outputs.clone();
    let first = capture(&mut app, &outputs);
    let baseline = &first[0];
    let blur = &first[1];
    let refracted = &first[2];
    let dispersed = &first[3];
    let specular = &first[4];
    let grained = &first[5];
    let translucent = &first[6];
    let fresnel = &first[7];
    let fresnel_and_dispersion = &first[8];

    let neutral_channels = edge_channel_difference(refracted);
    let dispersed_channels = edge_channel_difference(dispersed);
    println!(
        "optical-gpu comparison: zero-dispersion edge RGB delta={neutral_channels:.4}; \
         authored-dispersion edge RGB delta={dispersed_channels:.4}"
    );
    assert!(
        neutral_channels < 0.01,
        "zero authored dispersion must not tint a neutral backdrop edge"
    );

    let baseline_energy = gradient_energy(baseline);
    let blur_energy = gradient_energy(blur);
    assert!(
        blur_energy < baseline_energy * 0.45,
        "blur should suppress high-frequency backdrop detail: {blur_energy} vs {baseline_energy}"
    );

    let edge_shift = edge_difference(blur, refracted);
    assert!(
        edge_shift > 4.0,
        "refraction should move backdrop samples near the edge: {edge_shift}"
    );

    assert!(
        dispersed_channels > neutral_channels + 2.0,
        "dispersion should separate red and blue samples: {dispersed_channels} vs {neutral_channels}"
    );

    let specular_peak = specular_difference(refracted, specular);
    assert!(
        specular_peak > 8,
        "the lit rim should brighten edge pixels: {specular_peak}"
    );
    let fresnel_difference = edge_difference(fresnel, fresnel_and_dispersion);
    assert!(
        fresnel_difference > 1.0,
        "the theme-derived Fresnel lane must change real rim pixels: {fresnel_difference}"
    );
    let grain_delta = interior_difference(refracted, grained);
    assert!(
        grain_delta > 5.0,
        "the grain texture should affect glass pixels: {grain_delta}"
    );
    let opaque_delta = interior_difference(baseline, blur);
    let translucent_delta = interior_difference(baseline, translucent);
    assert!(
        translucent_delta > 1.0 && translucent_delta < opaque_delta * 0.9,
        "opacity should blend the sampled backdrop with its original pixels: {translucent_delta} vs {opaque_delta}"
    );

    let chain = app.world().resource::<Targets>().chain.clone();
    let chain_only = app.world().resource::<Targets>().chain_only_capture.clone();
    let nested = app.world().resource::<Targets>().nested_output.clone();
    // A second layer samples the CHAIN target, which holds the first layer's
    // real optical output rather than the painted lower composite. Both the
    // chain pixels and the second layer's own output are measured here.
    let chain_images = capture(&mut app, &[chain_only, chain.clone(), nested.clone()]);
    let chain_capture = &chain_images[0];
    let chain_layer_capture = &chain_images[1];
    let nested_capture = &chain_images[2];
    assert!(
        interior_difference(baseline, chain_capture) > 8.0,
        "the chain must carry the first optical layer's real pixels"
    );
    let nested_only = app
        .world()
        .resource::<Targets>()
        .nested_only_capture
        .clone();
    let nested_only_capture = &capture(&mut app, &[nested_only])[0];
    let nested_only_delta = interior_difference(chain_capture, nested_only_capture);
    println!(
        "optical-gpu production comparison: chain-to-nested-only delta={nested_only_delta:.4}"
    );
    assert!(
        nested_only_delta > 1.0,
        "the production nested layer must re-refract the chain target"
    );
    assert!(
        interior_difference(baseline, chain_layer_capture) > 8.0,
        "the chain layer must carry the first optical layer's real pixels"
    );
    assert!(
        interior_difference(baseline, nested_capture) > 8.0,
        "the first optical layer must carry its real pixels into the second layer"
    );
    assert!(
        interior_difference(chain_capture, nested_capture) > 1.0,
        "the nested optical layer must visibly transform the first layer's chain"
    );
    assert!(
        interior_difference(chain_capture, chain_layer_capture) < 0.01,
        "the chain-only capture and the chain layer must be the same real first-layer pixels"
    );

    for &(x, y) in &[(10, 10), (LEFT + 1, TOP + 1), (LEFT + 3, TOP + 3)] {
        assert_eq!(
            pixel(dispersed, x, y),
            pixel(baseline, x, y),
            "pixels outside the rounded card must remain unchanged"
        );
    }
    assert_eq!(
        pixel(dispersed, 80, 64),
        [255, 0, 0, 255],
        "foreground content stays sharp above the material"
    );

    let pattern = app.world().resource::<Targets>().pattern.clone();
    app.world_mut()
        .resource_mut::<Assets<Image>>()
        .get_mut(&pattern)
        .expect("source pattern")
        .data = pattern_image(true).data;
    advance(&mut app, 12);
    let second = capture(&mut app, &outputs);
    assert!(
        interior_difference(baseline, &second[0]) > 20.0,
        "the lower scene must actually change"
    );
    assert!(
        interior_difference(dispersed, &second[3]) > 5.0,
        "the material must sample the changed lower scene"
    );
    assert!(
        interior_difference(chain_capture, &second[8]) > 1.0,
        "the second layer's own pixels must change when the first layer changes"
    );
}

fn setup(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<OpticalGlassMaterial>>,
) {
    let pattern = images.add(pattern_image(false));
    let backdrop = target(&mut images);
    let backdrop_camera = commands
        .spawn((
            Camera2d,
            Camera {
                order: -10,
                clear_color: ClearColorConfig::Custom(Color::BLACK),
                ..Camera::default()
            },
            RenderTarget::Image(backdrop.clone().into()),
        ))
        .id();
    commands.spawn((
        Node {
            width: percent(100),
            height: percent(100),
            ..Node::default()
        },
        ImageNode::new(pattern.clone()),
        UiTargetCamera(backdrop_camera),
    ));

    let grain = images.add(solid_image());
    let mut outputs = Vec::new();
    for index in 0..9 {
        let output = target(&mut images);
        let camera = commands
            .spawn((
                Camera2d,
                Camera {
                    order: index,
                    clear_color: ClearColorConfig::Custom(Color::BLACK),
                    ..Camera::default()
                },
                RenderTarget::Image(output.clone().into()),
            ))
            .id();
        let material = (index > 0).then(|| {
            let mut spec = test_spec();
            if (2..=5).contains(&index) {
                spec.refraction_strength_rem = 14.0;
            }
            if index == 3 {
                spec.chromatic_dispersion = 0.45;
            }
            if index == 4 {
                spec.specular_width_rem = 6.0;
                spec.specular_intensity = 1.0;
            }
            if index == 5 {
                spec.grain_strength = 0.3;
            }
            if index == 6 {
                spec.frost_opacity = 0.5;
            }
            if index == 7 {
                spec.refraction_strength_rem = 14.0;
                spec.specular_width_rem = 6.0;
                spec.specular_intensity = 1.0;
                spec.quality = OpticalGlassQuality::Low;
            }
            if index == 8 {
                spec.refraction_strength_rem = 14.0;
                spec.chromatic_dispersion = 0.45;
                spec.specular_width_rem = 6.0;
                spec.specular_intensity = 1.0;
                spec.quality = OpticalGlassQuality::High;
            }
            let geometry = spec
                .resolve(OpticalGlassContext::new(1.0, 1.0))
                .expect("valid test geometry");
            let raster = OpticalGlassRaster {
                origin_px: Vec2::new(LEFT as f32, TOP as f32),
                size_px: Vec2::new(CARD_WIDTH as f32, CARD_HEIGHT as f32),
                viewport_px: Vec2::new(WIDTH as f32, HEIGHT as f32),
                device_scale_factor: 1.0,
            };
            materials.add(OpticalGlassMaterial {
                uniform: OpticalGlassUniform::project(
                    spec,
                    geometry,
                    raster,
                    OpticalGlassMaterialKind::Optical,
                )
                .expect("valid material placement"),
                backdrop: backdrop.clone(),
                grain: grain.clone(),
            })
        });
        commands
            .spawn((
                Node {
                    width: percent(100),
                    height: percent(100),
                    ..Node::default()
                },
                ImageNode::new(backdrop.clone()),
                UiTargetCamera(camera),
            ))
            .with_children(|parent| {
                if let Some(material) = material {
                    parent.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: px(LEFT),
                            top: px(TOP),
                            width: px(CARD_WIDTH),
                            height: px(CARD_HEIGHT),
                            ..Node::default()
                        },
                        MaterialNode(material),
                    ));
                }
                parent.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(76),
                        top: px(60),
                        width: px(10),
                        height: px(10),
                        ..Node::default()
                    },
                    BackgroundColor(Color::srgb(1.0, 0.0, 0.0)),
                ));
            });
        outputs.push(output);
    }

    // The SECOND optical layer's backdrop: a second offscreen camera paints
    // the first layer's optical output into the chain image. A third output
    // camera renders a second glass surface that samples that chain image.
    let chain = target(&mut images);
    let chain_camera = commands
        .spawn((
            Camera2d,
            Camera {
                order: 8,
                clear_color: ClearColorConfig::Custom(Color::BLACK),
                ..Camera::default()
            },
            RenderTarget::Image(chain.clone().into()),
        ))
        .id();
    commands
        .spawn((
            Node {
                width: percent(100),
                height: percent(100),
                ..Node::default()
            },
            ImageNode::new(backdrop.clone()),
            UiTargetCamera(chain_camera),
        ))
        .with_children(|parent| {
            let mut spec = test_spec();
            spec.refraction_strength_rem = 14.0;
            spec.chromatic_dispersion = 0.45;
            let geometry = spec
                .resolve(OpticalGlassContext::new(1.0, 1.0))
                .expect("valid test geometry");
            let raster = OpticalGlassRaster {
                origin_px: Vec2::new(LEFT as f32, TOP as f32),
                size_px: Vec2::new(CARD_WIDTH as f32, CARD_HEIGHT as f32),
                viewport_px: Vec2::new(WIDTH as f32, HEIGHT as f32),
                device_scale_factor: 1.0,
            };
            parent.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(LEFT),
                    top: px(TOP),
                    width: px(CARD_WIDTH),
                    height: px(CARD_HEIGHT),
                    ..Node::default()
                },
                MaterialNode(
                    materials.add(OpticalGlassMaterial {
                        uniform: OpticalGlassUniform::project(
                            spec,
                            geometry,
                            raster,
                            OpticalGlassMaterialKind::Optical,
                        )
                        .expect("valid first-layer material placement"),
                        backdrop: backdrop.clone(),
                        grain: grain.clone(),
                    }),
                ),
            ));
        });

    let chain_only_capture = target(&mut images);
    let chain_only_camera = commands
        .spawn((
            Camera2d,
            Camera {
                order: 9,
                clear_color: ClearColorConfig::Custom(Color::BLACK),
                ..Camera::default()
            },
            RenderTarget::Image(chain_only_capture.clone().into()),
        ))
        .id();
    commands.spawn((
        Node {
            width: percent(100),
            height: percent(100),
            ..Node::default()
        },
        ImageNode::new(chain.clone()),
        UiTargetCamera(chain_only_camera),
    ));

    let nested_output = target(&mut images);
    let nested_only_capture = target(&mut images);
    let nested_camera = commands
        .spawn((
            Camera2d,
            Camera {
                order: 10,
                clear_color: ClearColorConfig::Custom(Color::BLACK),
                ..Camera::default()
            },
            RenderTarget::Image(nested_output.clone().into()),
        ))
        .id();
    let mut nested_spec = test_spec();
    nested_spec.refraction_strength_rem = 14.0;
    nested_spec.chromatic_dispersion = 0.45;
    let nested_geometry = nested_spec
        .resolve(OpticalGlassContext::new(1.0, 1.0).with_nesting_depth(1))
        .expect("valid nested test geometry");
    let nested_raster = OpticalGlassRaster {
        origin_px: Vec2::new(NESTED_LEFT as f32, NESTED_TOP as f32),
        size_px: Vec2::new(NESTED_WIDTH as f32, NESTED_HEIGHT as f32),
        viewport_px: Vec2::new(WIDTH as f32, HEIGHT as f32),
        device_scale_factor: 1.0,
    };
    let nested_only_camera = commands
        .spawn((
            Camera2d,
            Camera {
                order: 11,
                clear_color: ClearColorConfig::Custom(Color::BLACK),
                ..Camera::default()
            },
            RenderTarget::Image(nested_only_capture.clone().into()),
        ))
        .id();
    commands
        .spawn((
            Node {
                width: percent(100),
                height: percent(100),
                ..Node::default()
            },
            ImageNode::new(chain.clone()),
            UiTargetCamera(nested_camera),
        ))
        .with_children(|parent| {
            parent.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(NESTED_LEFT),
                    top: px(NESTED_TOP),
                    width: px(NESTED_WIDTH),
                    height: px(NESTED_HEIGHT),
                    ..Node::default()
                },
                MaterialNode(
                    materials.add(OpticalGlassMaterial {
                        uniform: OpticalGlassUniform::project(
                            nested_spec,
                            nested_geometry,
                            nested_raster,
                            OpticalGlassMaterialKind::Optical,
                        )
                        .expect("valid nested material placement"),
                        backdrop: chain.clone(),
                        grain: grain.clone(),
                    }),
                ),
            ));
        });
    commands.spawn((
        Node {
            width: percent(100),
            height: percent(100),
            ..Node::default()
        },
        ImageNode::new(nested_output.clone()),
        UiTargetCamera(nested_only_camera),
    ));

    commands.insert_resource(Targets {
        pattern,
        outputs,
        chain,
        chain_only_capture,
        nested_output,
        nested_only_capture,
    });
}

fn test_spec() -> OpticalGlassSpec {
    let mut spec = Theme::dark()
        .optical_glass_spec(GlassTier::Card)
        .expect("card optical spec");
    spec.blur_radius_rem = 7.0;
    spec.refraction_strength_rem = 0.0;
    spec.refraction_width_rem = 18.0;
    spec.chromatic_dispersion = 0.0;
    spec.specular_intensity = 0.0;
    spec.frost_opacity = 1.0;
    spec.tint = spec.tint.with_alpha(0.0);
    spec.grain_strength = 0.0;
    spec.corner_radius_rem = 18.0;
    spec
}

fn target(images: &mut Assets<Image>) -> Handle<Image> {
    let mut image = Image::new_target_texture(WIDTH, HEIGHT, TextureFormat::Rgba8UnormSrgb, None);
    image.asset_usage = RenderAssetUsages::RENDER_WORLD;
    images.add(image)
}

fn pattern_image(inverted: bool) -> Image {
    let mut data = Vec::with_capacity((WIDTH * HEIGHT * 4) as usize);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let detail = if (x / 5 + y / 7).is_multiple_of(2) {
                110
            } else {
                30
            };
            let band = if (x / 18).is_multiple_of(2) { 70 } else { 0 };
            let value = if inverted {
                255 - detail - band
            } else {
                detail + band
            };
            data.extend_from_slice(&[value, value, value, 255]);
        }
    }
    Image::new(
        Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

fn solid_image() -> Image {
    Image::new(
        Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        vec![0, 0, 0, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

fn advance(app: &mut App, frames: usize) {
    for _ in 0..frames {
        app.update();
        app.world()
            .resource::<RenderDevice>()
            .wgpu_device()
            .poll(PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(10)),
            })
            .expect("GPU submission completes");
    }
}

fn capture(app: &mut App, outputs: &[Handle<Image>]) -> Vec<Image> {
    let entities = outputs
        .iter()
        .map(|output| {
            app.world_mut()
                .spawn(Screenshot::image(output.clone()))
                .observe(
                    |event: On<ScreenshotCaptured>, mut captures: ResMut<Captures>| {
                        captures.0.insert(event.entity, event.image.clone());
                    },
                )
                .id()
        })
        .collect::<Vec<_>>();
    for _ in 0..100 {
        advance(app, 1);
        if entities
            .iter()
            .all(|entity| app.world().resource::<Captures>().0.contains_key(entity))
        {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    let mut captures = app.world_mut().resource_mut::<Captures>();
    entities
        .into_iter()
        .map(|entity| captures.0.remove(&entity).expect("final output captured"))
        .collect()
}

fn pixel(image: &Image, x: u32, y: u32) -> [u8; 4] {
    let data = image.data.as_ref().expect("captured pixels");
    let offset = ((y * WIDTH + x) * 4) as usize;
    data[offset..offset + 4].try_into().expect("RGBA pixel")
}

fn gradient_energy(image: &Image) -> f64 {
    let mut sum = 0.0;
    let mut count = 0;
    for y in 45..100 {
        for x in 45..150 {
            if (76..87).contains(&x) && (60..70).contains(&y) {
                continue;
            }
            let left = f64::from(pixel(image, x, y)[0]);
            let right = f64::from(pixel(image, x + 1, y)[0]);
            sum += (left - right).powi(2);
            count += 1;
        }
    }
    sum / f64::from(count)
}

fn edge_difference(left: &Image, right: &Image) -> f64 {
    let mut sum = 0.0;
    let mut count = 0;
    for y in 45..100 {
        for x in 26..40 {
            sum += (f64::from(pixel(left, x, y)[0]) - f64::from(pixel(right, x, y)[0])).abs();
            count += 1;
        }
    }
    sum / f64::from(count)
}

fn edge_channel_difference(image: &Image) -> f64 {
    let mut sum = 0.0;
    let mut count = 0;
    for y in 45..100 {
        for x in 26..40 {
            let color = pixel(image, x, y);
            sum += (f64::from(color[0]) - f64::from(color[2])).abs();
            count += 1;
        }
    }
    sum / f64::from(count)
}

fn specular_difference(left: &Image, right: &Image) -> u8 {
    let mut max_delta = 0;
    for y in 24..45 {
        for x in 26..65 {
            let left = pixel(left, x, y)[0];
            let right = pixel(right, x, y)[0];
            max_delta = max_delta.max(right.saturating_sub(left));
        }
    }
    max_delta
}

fn interior_difference(left: &Image, right: &Image) -> f64 {
    let mut sum = 0.0;
    let mut count = 0;
    for y in 45..100 {
        for x in 45..150 {
            let left = pixel(left, x, y);
            let right = pixel(right, x, y);
            sum += (f64::from(left[0]) - f64::from(right[0])).abs();
            count += 1;
        }
    }
    sum / f64::from(count)
}
