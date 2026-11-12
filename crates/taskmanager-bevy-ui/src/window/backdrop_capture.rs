//! Primary-window presentation and offscreen backdrop-camera ownership.
//!
//! The interactive shell stays on the primary-window camera. The isolated
//! backdrop root carries an explicit `UiTargetCamera`, so Bevy UI can render
//! it into an image without stealing pointer routing from the shell. The
//! invalidation controller renders the offscreen target for exactly one frame
//! per lower-composite change while an attached optical material exists; a
//! typed fallback surface leaves the optical camera dormant.

use bevy::app::App;
use bevy::asset::Assets;
use bevy::camera::{
    Camera, Camera2d, ClearColorConfig, ComputedCameraValues, ImageRenderTarget, RenderTarget,
    RenderTargetInfo,
};
use bevy::ecs::change_detection::DetectChanges;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::observer::On;
use bevy::ecs::query::{With, Without};
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, Query, Res, ResMut, SystemParam};
use bevy::ecs::template::template;
use bevy::image::Image;
use bevy::math::UVec2;
use bevy::picking::Pickable;
use bevy::render::render_resource::{Extent3d, TextureFormat};
use bevy::scene::{CommandsSceneExt, bsn};
use bevy::ui::UiTargetCamera;
use bevy::ui::prelude::{BackgroundColor, Node, percent, px};
use bevy::window::{PrimaryWindow, Window, WindowRef};

use crate::app::Route;
use crate::drain::ShellProjectionFolded;
use crate::glass::optical::{OpticalGlassBinding, OpticalGlassSurface, OpticalNestingDepths};
use crate::palette::UiPalette;
use crate::window::WindowPalette;

/// The painted lower composite's camera order.
///
/// Cameras render in ascending order, so the capture has to paint BEFORE the
/// chain that samples it, and both before the presentation that draws the
/// window.
const CAPTURE_CAMERA_ORDER: isize = -3;

/// The second optical layer's camera order.
///
/// The chain samples the painted lower composite, so it renders after the
/// capture that produced it.
const CHAIN_CAMERA_ORDER: isize = -2;

/// The interactive shell's camera order.
const PRESENTATION_CAMERA_ORDER: isize = -1;

/// The painted lower composite's camera.
type CaptureCameraQuery<'w, 's> =
    Query<'w, 's, (&'static mut RenderTarget, &'static mut Camera), With<BackdropCaptureCamera>>;

/// The second optical layer's camera.
type ChainCameraQuery<'w, 's> = Query<
    'w,
    's,
    (&'static mut RenderTarget, &'static mut Camera),
    (With<BackdropChainCamera>, Without<BackdropCaptureCamera>),
>;

/// The interactive shell's camera.
type PresentationCameraQuery<'w, 's> = Query<
    'w,
    's,
    &'static mut Camera,
    (
        With<PresentationCamera>,
        (Without<BackdropCaptureCamera>, Without<BackdropChainCamera>),
    ),
>;

/// The only camera that presents interactive UI to the primary window.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct PresentationCamera;

/// Camera that renders the non-interactive backdrop into an offscreen image.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct BackdropCaptureCamera;

/// Camera that renders the SECOND optical layer's backdrop into its own
/// offscreen image.
///
/// The second layer's backdrop is not the painted lower composite: it is
/// the FIRST optical layer's rendered pixels, which live in their own
/// offscreen image. This camera is the one that samples that image and
/// republishes it as the composite an overlay (a tooltip, a popover, a
/// modal) actually sees.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct BackdropChainCamera;

/// Root of the offscreen backdrop tree. This tree must never contain
/// interactive UI; keeping it separate prevents the backdrop camera from
/// stealing the shell's pointer target.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct BackdropCaptureRoot;

/// The live offscreen backdrop target and the primary-window dimensions it
/// mirrors. The image is resized only when the physical window size or scale
/// factor changes.
#[derive(Resource, Clone, Debug)]
pub(crate) struct BackdropCaptureTarget {
    pub(crate) image: bevy::asset::Handle<Image>,
    pub(crate) camera: Entity,
    pub(crate) physical_size: UVec2,
    pub(crate) scale_factor: f32,
}

/// The live SECOND optical layer's backdrop target.
///
/// This target is the composite an overlay samples: it carries the page
/// canvas, the painted lower surfaces, AND the first optical layer's own
/// rendered pixels. A glass overlay that samples this image therefore
/// refracts the real first-layer glass underneath it instead of a painted
/// stand-in for it.
#[derive(Resource, Clone, Debug)]
pub(crate) struct BackdropChainTarget {
    pub(crate) image: bevy::asset::Handle<Image>,
    pub(crate) camera: Entity,
    /// The image this chain samples: the first optical layer's own output.
    pub(crate) source: bevy::asset::Handle<Image>,
    pub(crate) physical_size: UVec2,
    pub(crate) scale_factor: f32,
}

/// Monotonic revision of the lower composite behind the glass. Fold, resize,
/// palette and route invalidations bump the counter; the capture controller
/// compares it against the revision the offscreen image last mirrored.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct BackdropRevision(pub(crate) u64);

impl BackdropRevision {
    pub(crate) fn bump(&mut self) {
        self.0 = self.0.wrapping_add(1);
    }
}

/// The revision the offscreen image last captured. `None` until the first
/// attached material always triggers one frame.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub(crate) struct BackdropCaptureState {
    pub(crate) captured: Option<u64>,
}

/// Spawn both cameras and the isolated backdrop root when an image store is
/// available. Headless compositions without an image store still get the
/// presentation camera but report no capture target.
pub(super) fn spawn(
    palette: &UiPalette,
    window: Option<&Window>,
    images: Option<ResMut<Assets<Image>>>,
    commands: &mut Commands,
) {
    let (presentation_size, presentation_scale) = capture_target_geometry(window);
    let _presentation_camera = commands
        .spawn_scene(bsn! {
            Camera2d
            Camera {
                order: PRESENTATION_CAMERA_ORDER,
                computed: {
                    ComputedCameraValues {
                    target_info: Some(RenderTargetInfo {
                        physical_size: presentation_size,
                        scale_factor: presentation_scale,
                    }),
                    ..Default::default()
                    }
                }
            }
            RenderTarget::Window(WindowRef::Primary)
            PresentationCamera
        })
        .id();

    let Some(mut images) = images else {
        return;
    };
    let (physical_size, scale_factor) = capture_target_geometry(window);
    let image = Image::new_target_texture(
        physical_size.x,
        physical_size.y,
        TextureFormat::Rgba8UnormSrgb,
        None,
    );
    let image = images.add(image);
    let camera_image = image.clone();
    let capture_camera = commands
        .spawn_scene(bsn! {
            Camera2d
            Camera {
                order: CAPTURE_CAMERA_ORDER,
                is_active: false,
                clear_color: { ClearColorConfig::Custom(palette.canvas) },
                computed: {
                    ComputedCameraValues {
                    target_info: Some(RenderTargetInfo {
                        physical_size,
                        scale_factor,
                    }),
                    ..Default::default()
                    }
                }
            }
            template(move |_| {
                Ok(RenderTarget::Image(ImageRenderTarget {
                    handle: camera_image.clone(),
                    scale_factor,
                }))
            })
            BackdropCaptureCamera
        })
        .id();
    // This root is deliberately not a child of AppShellRoot: the shell remains
    // the only interactive UI tree and keeps its presentation-camera target.
    // The scaffold wears the page canvas, the same surface the real cards sit
    // on, because it IS the backdrop an optical panel samples: a panel whose
    // backdrop is a different color than the surface behind it reads as a
    // hole punched in the page rather than as glass resting on it.
    commands.spawn_scene(bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            min_width: px(0.0),
            min_height: px(0.0),
        }
        BackgroundColor({ palette.canvas })
        Pickable::IGNORE
        UiTargetCamera({ capture_camera })
        BackdropCaptureRoot
    });
    commands.insert_resource(BackdropCaptureTarget {
        image: image.clone(),
        camera: capture_camera,
        physical_size,
        scale_factor,
    });

    // The SECOND optical layer's backdrop. It samples the FIRST layer's
    // output image, so an overlay that refracts this image refracts the
    // product's real first-layer glass rather than a painted stand-in for it.
    let chain_image = Image::new_target_texture(
        physical_size.x,
        physical_size.y,
        TextureFormat::Rgba8UnormSrgb,
        None,
    );
    let chain_image = images.add(chain_image);
    let chain_camera_image = chain_image.clone();
    let chain_camera = commands
        .spawn_scene(bsn! {
            Camera2d
            Camera {
                order: CHAIN_CAMERA_ORDER,
                is_active: false,
                clear_color: { ClearColorConfig::Custom(palette.canvas) },
                computed: {
                    ComputedCameraValues {
                    target_info: Some(RenderTargetInfo {
                        physical_size,
                        scale_factor,
                    }),
                    ..Default::default()
                    }
                }
            }
            template(move |_| {
                Ok(RenderTarget::Image(ImageRenderTarget {
                    handle: chain_camera_image.clone(),
                    scale_factor,
                }))
            })
            BackdropChainCamera
        })
        .id();
    commands.insert_resource(BackdropChainTarget {
        image: chain_image,
        camera: chain_camera,
        source: image,
        physical_size,
        scale_factor,
    });
}

/// Resolve the offscreen target from the primary window's physical size.
/// A missing/temporarily-zero window still gets a valid 1x1 target; the
/// resize system replaces it as soon as authoritative dimensions are
/// available.
fn capture_target_geometry(window: Option<&Window>) -> (UVec2, f32) {
    let mut physical_size = window.map(Window::physical_size).unwrap_or(UVec2::ONE);
    physical_size.x = physical_size.x.max(1);
    physical_size.y = physical_size.y.max(1);
    let scale_factor = window
        .map(Window::scale_factor)
        .filter(|scale| scale.is_finite() && *scale > 0.0)
        .unwrap_or(1.0);
    (physical_size, scale_factor)
}

/// Keep the offscreen target in lockstep with the primary window without
/// rebuilding the whole UI tree every frame. The image is resized only on a
/// physical-size change; the camera target's scale factor is updated only on
/// a DPI change.
pub(super) fn sync_backdrop_capture(access: BackdropCaptureAccess<'_, '_>) {
    let BackdropCaptureAccess {
        windows,
        images,
        target,
        chain,
        mut revision,
        mut cameras,
        mut chain_cameras,
        mut presentation_cameras,
    } = access;
    let Some(mut target) = target else {
        return;
    };
    let Some(mut images) = images else {
        return;
    };
    let Some(window) = windows.iter().next() else {
        return;
    };
    let (physical_size, scale_factor) = capture_target_geometry(Some(window));
    let size_changed = physical_size != target.physical_size;
    let scale_changed = (scale_factor - target.scale_factor).abs() > f32::EPSILON;
    if !size_changed && !scale_changed {
        return;
    }
    if let Some(revision) = revision.as_deref_mut() {
        revision.bump();
    }
    if size_changed && let Some(mut image) = images.get_mut(&target.image) {
        image.resize(Extent3d {
            width: physical_size.x,
            height: physical_size.y,
            depth_or_array_layers: 1,
        });
    }
    if let Ok((mut render_target, mut camera)) = cameras.get_mut(target.camera) {
        if scale_changed && let RenderTarget::Image(image_target) = &mut *render_target {
            image_target.scale_factor = scale_factor;
        }
        camera.computed.target_info = Some(RenderTargetInfo {
            physical_size,
            scale_factor,
        });
    }
    if let Some(mut chain) = chain
        && let Ok((mut render_target, mut camera)) = chain_cameras.get_mut(chain.camera)
    {
        if scale_changed && let RenderTarget::Image(image_target) = &mut *render_target {
            image_target.scale_factor = scale_factor;
        }
        camera.computed.target_info = Some(RenderTargetInfo {
            physical_size,
            scale_factor,
        });
        if size_changed && let Some(mut image) = images.get_mut(&chain.image) {
            image.resize(Extent3d {
                width: physical_size.x,
                height: physical_size.y,
                depth_or_array_layers: 1,
            });
        }
        chain.physical_size = physical_size;
        chain.scale_factor = scale_factor;
    }
    for mut camera in &mut presentation_cameras {
        camera.computed.target_info = Some(RenderTargetInfo {
            physical_size,
            scale_factor,
        });
    }
    target.physical_size = physical_size;
    target.scale_factor = scale_factor;
}

/// The capture targets, the cameras that render them, and the window they
/// mirror. One named bundle keeps the system under Clippy's argument ceiling
/// without hiding a whole `World`.
#[derive(SystemParam)]
pub(super) struct BackdropCaptureAccess<'w, 's> {
    windows: Query<'w, 's, &'static Window, With<PrimaryWindow>>,
    images: Option<ResMut<'w, Assets<Image>>>,
    target: Option<ResMut<'w, BackdropCaptureTarget>>,
    chain: Option<ResMut<'w, BackdropChainTarget>>,
    revision: Option<ResMut<'w, BackdropRevision>>,
    cameras: CaptureCameraQuery<'w, 's>,
    chain_cameras: ChainCameraQuery<'w, 's>,
    presentation_cameras: PresentationCameraQuery<'w, 's>,
}

/// Fold observer: live telemetry landed in the projection, so the lower
/// composite the glass samples may have changed.
pub(super) fn bump_backdrop_revision(
    _trigger: On<ShellProjectionFolded>,
    revision: Option<ResMut<BackdropRevision>>,
) {
    if let Some(mut revision) = revision {
        revision.bump();
    }
}

/// The backdrop invalidation controller: render the offscreen target for
/// exactly one frame whenever an attached optical material exists and the
/// lower composite may have changed — a fold, a palette or route change, a
/// window resize, or the first-ever surface. A FROSTED material samples the
/// backdrop exactly like the complete one, so it wakes the camera too; a
/// painted or opaque fallback has no material for the shader to sample, so
/// the controller stays dormant and the camera never renders.
pub(super) fn drive_backdrop_capture(access: BackdropDriveAccess<'_, '_>) {
    let BackdropDriveAccess {
        revision,
        mut state,
        palette,
        route,
        depths,
        surfaces,
        mut cameras,
        mut chain_cameras,
    } = access;
    let Some(revision) = revision else {
        return;
    };
    let stale = state.captured != Some(revision.0)
        || palette.is_some_and(|palette| palette.is_changed())
        || route.is_some_and(|route| route.is_changed());
    let capture = stale
        && surfaces
            .iter()
            .any(|(_, binding)| binding.attached_material().is_some());
    if capture {
        state.captured = Some(revision.0);
    }
    for mut camera in &mut cameras {
        if camera.is_active != capture {
            camera.is_active = capture;
        }
    }
    // The chain camera renders only when a real second optical layer
    // exists. Its output is the composite a nested surface samples, so a
    // composition without a stacked optical surface stays dormant.
    let chain_capture = capture && depths.is_some_and(|depths| depths.has_nested_surface());
    for mut camera in &mut chain_cameras {
        if camera.is_active != chain_capture {
            camera.is_active = chain_capture;
        }
    }
}

/// The invalidation controller's inputs. One named bundle keeps the system
/// under Clippy's argument ceiling without hiding a whole `World`.
#[derive(SystemParam)]
pub(super) struct BackdropDriveAccess<'w, 's> {
    revision: Option<Res<'w, BackdropRevision>>,
    state: ResMut<'w, BackdropCaptureState>,
    palette: Option<Res<'w, WindowPalette>>,
    route: Option<Res<'w, Route>>,
    depths: Option<Res<'w, OpticalNestingDepths>>,
    surfaces: Query<'w, 's, (Entity, &'static OpticalGlassBinding), With<OpticalGlassSurface>>,
    cameras: Query<'w, 's, &'static mut Camera, With<BackdropCaptureCamera>>,
    chain_cameras: Query<
        'w,
        's,
        &'static mut Camera,
        (With<BackdropChainCamera>, Without<BackdropCaptureCamera>),
    >,
}

/// Register the invalidation resources and the fold observer. The drive
/// system is chained right after the resize sync in the window composition.
pub(crate) fn register(app: &mut App) {
    app.init_resource::<BackdropRevision>()
        .init_resource::<BackdropCaptureState>()
        .add_observer(bump_backdrop_revision);
}
