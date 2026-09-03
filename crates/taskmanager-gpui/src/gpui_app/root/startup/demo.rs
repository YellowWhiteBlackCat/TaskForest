//! Deterministic GPUI demo composition without native host I/O.

use super::super::{RootView, i18n, responsive};
use super::{requested_window_decorations, resolve_app_id};
use crate::gpui_app::chrome::WindowDecorationsPreference;
use crate::gpui_app::theme::detect;
use gpui::{
    App, AppContext, Bounds, Entity, TitlebarOptions, WindowBounds, WindowOptions, point, px, size,
};
use taskmanager_app_host::WindowPresentation;
use taskmanager_assets::product;
use taskmanager_core::core::appearance::DesktopAppearance;
use taskmanager_theme::Theme;
use taskmanager_ui::theme_binding::background_appearance;
use tracing::{error, warn};

/// Open one normal desktop window backed only by deterministic typed fixture
/// data. The production startup path is deliberately not reused here because
/// it owns configuration, native appearance, platform collection, history
/// persistence, single-instance and tray lifetimes.
pub(crate) fn init_demo(cx: &mut App, custom_app_id: Option<String>) {
    taskmanager_ui::init(cx);
    if let Err(font_error) = cx
        .text_system()
        .add_fonts(taskmanager_assets::embedded_fonts())
    {
        warn!(%font_error, "embedded demo font registration failed; falling back to system fonts");
    }

    i18n::set_language(i18n::detect_language());
    let mut theme: Theme = detect(DesktopAppearance::default());
    theme.window_transparent = cfg!(target_os = "linux");
    let app_id = resolve_app_id(custom_app_id);
    let presentation = WindowPresentation::standalone();
    let options = WindowOptions {
        titlebar: Some(TitlebarOptions {
            title: Some(product::GPUI_NAME.into()),
            ..Default::default()
        }),
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: point(px(120.0), px(80.0)),
            size: responsive::initial_window_size(),
        })),
        window_decorations: Some(requested_window_decorations(
            WindowDecorationsPreference::default(),
        )),
        window_background: background_appearance(&theme),
        app_id: Some(app_id),
        window_min_size: Some(size(px(responsive::MIN_WIDTH), px(responsive::MIN_HEIGHT))),
        presentation: crate::window_presentation::to_gpui(&presentation),
        ..Default::default()
    };

    let window_result = cx.open_window(options, move |window, cx| {
        window.set_window_title(product::GPUI_NAME);
        let entity: Entity<RootView> = cx.new(|cx| RootView::new_demo(theme, cx));
        entity
    });
    if let Err(error) = window_result {
        error!(%error, "demo GPUI window failed to open");
        cx.quit();
    }
    cx.activate(true);
}
