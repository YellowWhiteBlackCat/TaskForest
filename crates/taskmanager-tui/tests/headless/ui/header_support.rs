//! Test-only header tab-text adapter: the default-theme spelling the
//! header projection tests pin.

use super::header_tab_text_with_theme;
use crate::TuiTheme;
use taskmanager_ui_contract::IconId;

pub(crate) fn header_tab_text(icon: IconId, label: &str, shortcut: &str, width: u16) -> String {
    header_tab_text_with_theme(icon, label, shortcut, width, TuiTheme::default())
}
