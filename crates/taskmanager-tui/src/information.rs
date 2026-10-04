//! Application About and an independent frozen system-information review.

use crate::command_palette::{TuiSurfaceScope, surface_protocol_action};
use crate::{TuiApp, TuiSurface, TuiSurfaceKind};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use taskmanager_application::PlatformEffect;
use taskmanager_application::i18n::t;
use taskmanager_assets::product;
use taskmanager_core::core::hardware::HardwareInfo;
use taskmanager_shell::presentation::about::metadata;
use taskmanager_shell::presentation::system_information::{
    SystemInformationGroup, copy_all_text, groups,
};
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource};

#[derive(Clone, Debug, Default)]
pub(crate) struct AboutTargetView {
    pub(crate) scroll: usize,
}
#[derive(Clone, Debug)]
pub(crate) struct SystemInformationTargetView {
    pub(crate) facts: Vec<SystemInformationGroup>,
    pub(crate) scroll: usize,
}

impl TuiApp {
    pub(crate) fn open_system_information(&mut self) {
        let facts = groups(
            self.projection()
                .hardware
                .as_ref()
                .unwrap_or(&HardwareInfo::default()),
            self.observed_appearance.unwrap_or_default(),
        );
        self.open_local_surface(TuiSurface::SystemInformation(SystemInformationTargetView {
            facts,
            scroll: 0,
        }));
    }
    pub(crate) fn information_copy_payload(&self) -> Option<String> {
        match self.local_surface()? {
            TuiSurface::About(_) => Some(
                metadata(
                    env!("CARGO_PKG_VERSION"),
                    product::LICENSE_SPDX,
                    product::REPOSITORY_URL,
                )
                .details_text(),
            ),
            TuiSurface::SystemInformation(view) => Some(copy_all_text(&view.facts)),
            _ => None,
        }
    }
    pub(crate) fn copy_information_to(&mut self, writer: &mut impl std::io::Write) {
        let Some(text) = self.information_copy_payload() else {
            return;
        };
        let (severity, message) = match crate::clipboard::write_clipboard(writer, &text) {
            Ok(()) => (FeedbackSeverity::Success, t("common.copied").to_owned()),
            Err(error) => (FeedbackSeverity::Error, error.to_string()),
        };
        self.report_notice(
            FeedbackSource::Clipboard,
            severity,
            FeedbackLifecycle::SHORT,
            message,
        );
    }
}

pub(crate) fn handle_key(app: &mut TuiApp, key: KeyEvent) -> Option<PlatformEffect> {
    if key
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
    {
        return None;
    }
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => app.close_local_overlays(),
        KeyCode::Char(character) => {
            let scope = if app.local_surface_kind() == Some(TuiSurfaceKind::About) {
                TuiSurfaceScope::About
            } else {
                TuiSurfaceScope::SystemInformation
            };
            if let Some(action) = surface_protocol_action(scope, character) {
                return app.run_surface_protocol_action(action);
            }
        }
        KeyCode::Up
        | KeyCode::Down
        | KeyCode::PageUp
        | KeyCode::PageDown
        | KeyCode::Home
        | KeyCode::End => {
            let scroll = match app.local_surface_mut() {
                Some(TuiSurface::About(view)) => &mut view.scroll,
                Some(TuiSurface::SystemInformation(view)) => &mut view.scroll,
                _ => return None,
            };
            *scroll = match key.code {
                KeyCode::Up => scroll.saturating_sub(1),
                KeyCode::Down => scroll.saturating_add(1),
                KeyCode::PageUp => scroll.saturating_sub(5),
                KeyCode::PageDown => scroll.saturating_add(5),
                KeyCode::Home => 0,
                KeyCode::End => usize::MAX,
                _ => *scroll,
            };
        }
        _ => {}
    }
    None
}
