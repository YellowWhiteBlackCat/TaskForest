//! Shared support for the TUI acceptance battery (sizes × locales × raw keys).
//!
//! The battery renders whole frames through the public [`crate::ui::render`]
//! entry on the deterministic demo fixture and asserts on painted text only —
//! behavior, never source text. Every render pins its language through the
//! shared [`crate::ui::test_support::LANG_TEST_GUARD`] so the process-global
//! `t()` cannot leak translations across concurrently running tests.

use std::sync::OnceLock;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use taskmanager_application::i18n::{self, Language};
use taskmanager_application::{AppAction, AppPage};

use crate::TuiApp;

pub(crate) const REFERENCE_WIDTH: u16 = 120;
pub(crate) const REFERENCE_HEIGHT: u16 = 36;

/// Every distinct first segment of the embedded i18n catalogs
/// (`locales/en.json` / `locales/zh.json`), as of the 2026-08 acceptance
/// battery. A raw key that leaks into a frame always *starts* with one of
/// these literal prefixes followed by more dotted lowercase segments, so this
/// exact list — not a heuristic like "contains a dot" — is what the scan
/// matches. Normal copy that merely contains a period ("page. The next…",
/// "1.2.3", "/etc/system.d") never satisfies the full key shape.
pub(crate) const RAW_KEY_PREFIXES: &[&str] = &[
    "about.",
    "alert.",
    "alerts.",
    "battery.",
    "chrome.",
    "command.",
    "common.",
    "confirm.",
    "containers.",
    "cpu.",
    "dashboard.",
    "device.",
    "diagnostics.",
    "dialog.",
    "disk.",
    "empty.",
    "events.",
    "fallback.",
    "fan.",
    "feedback.",
    "first_run.",
    "footer.",
    "gpu.",
    "graph.",
    "hardware.",
    "health.",
    "help.",
    "hint.",
    "history.",
    "mem.",
    "menu.",
    "net.",
    "network.",
    "npu.",
    "page.",
    "perf.",
    "proc.",
    "proc_control.",
    "proc_insights.",
    "prop.",
    "saved_views.",
    "search.",
    "settings.",
    "sidebar.",
    "source.",
    "startup.",
    "svc.",
    "system.",
    "system_about.",
    "tab.",
    "tooltip.",
    "tray.",
    "tui.",
    "users.",
];

/// Every catalog key from the embedded English locale, extracted at test
/// runtime from the same file the production i18n layer embeds. A leak means a
/// call site painted a key literal without wrapping it in `t()` — or a key
/// missing from the catalog fell back to itself — so the *exact* key set is
/// the strongest false-positive-free oracle.
pub(crate) fn catalog_keys() -> &'static [&'static str] {
    static KEYS: OnceLock<&'static [&'static str]> = OnceLock::new();
    KEYS.get_or_init(|| {
        let text: &'static str = include_str!("../../../../../../locales/en.json");
        let bytes = text.as_bytes();
        let mut keys: Vec<&'static str> = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] != b'"' {
                i += 1;
                continue;
            }
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() {
                match bytes[j] {
                    b'\\' => j += 2,
                    b'"' => break,
                    _ => j += 1,
                }
            }
            let token = &text[start..j.min(bytes.len())];
            let mut k = j + 1;
            while k < bytes.len() && bytes[k].is_ascii_whitespace() {
                k += 1;
            }
            // The catalog is one flat object: every `"token"` followed by a
            // colon is a key. Values are followed by `,` or `}`.
            if k < bytes.len() && bytes[k] == b':' {
                keys.push(token);
            }
            i = j + 1;
        }
        keys.sort_unstable();
        keys.dedup();
        Box::leak(keys.into_boxed_slice())
    })
}

/// The dotted lowercase identifier runs painted in `frame` that start with a
/// known catalog prefix — the raw-key leak shape. A run only matches when the
/// text after the prefix is itself a dotted lowercase identifier, so English
/// sentence periods, decimal numbers and filesystem paths cannot false-positive.
pub(crate) fn key_shaped_runs(frame: &str) -> Vec<String> {
    let mut runs = Vec::new();
    let mut run = String::new();
    // The trailing space flushes a run that ends at the frame's last cell.
    for ch in frame.chars().chain(std::iter::once(' ')) {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '.' {
            run.push(ch);
        } else if is_raw_key_shape(&run) {
            runs.push(std::mem::take(&mut run));
        } else {
            run.clear();
        }
    }
    runs
}

fn is_raw_key_shape(run: &str) -> bool {
    RAW_KEY_PREFIXES.iter().any(|prefix| {
        run.starts_with(prefix)
            && run[prefix.len()..].split('.').all(|segment| {
                !segment.is_empty()
                    && segment
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b == b'_' || b.is_ascii_digit())
            })
    })
}

/// The battery oracle: the painted frame must not contain any exact catalog
/// key (a call site that skipped `t()`) nor any key-shaped run over the exact
/// prefix list (a key missing from both catalogs rendering as itself).
pub(crate) fn assert_frame_has_no_raw_catalog_keys(frame: &str, surface: &str, language: Language) {
    let exact: Vec<&str> = catalog_keys()
        .iter()
        .copied()
        .filter(|key| frame.contains(key))
        .collect();
    let shaped = key_shaped_runs(frame);
    assert!(
        exact.is_empty() && shaped.is_empty(),
        "raw i18n catalog keys leaked onto the {surface} surface ({language:?}): \
         exact={exact:?} key-shaped={shaped:?}"
    );
}

/// Render one frame at `language` and hand the painted text to `verify` while
/// the language guard is still held (so `t()` inside `verify` resolves
/// against the same language the frame was painted with).
pub(crate) fn with_frame_in_language<R>(
    app: &TuiApp,
    width: u16,
    height: u16,
    language: Language,
    verify: impl FnOnce(&str) -> R,
) -> R {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    i18n::set_language(language);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| crate::ui::render(frame, app, crate::TuiTheme::default()))
        .expect("draw");
    verify(&terminal.backend().to_string())
}

/// Convenience wrapper: render and return the painted text.
pub(crate) fn frame_in_language(
    app: &TuiApp,
    width: u16,
    height: u16,
    language: Language,
) -> String {
    with_frame_in_language(app, width, height, language, std::borrow::ToOwned::to_owned)
}

/// The frame's body region text. `render` lays out header(Length 4) /
/// body(Min 8) / footer(Length 3), so the body is frame rows `4..height-3`.
/// Duplicating the chrome sizes here mirrors the visibility net's extractor:
/// when `render` changes its layout contract this must be revisited.
pub(crate) fn body_text(frame: &str, height: u16) -> String {
    let body_rows = height.saturating_sub(4 + 3);
    frame
        .lines()
        .skip(4)
        .take(usize::from(body_rows))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Rows that paint at least one non-whitespace cell.
pub(crate) fn visible_row_count(text: &str) -> usize {
    text.lines().filter(|line| !line.trim().is_empty()).count()
}

/// One overlay surface of the acceptance battery: a named opener that must
/// leave the app owning that surface. `popup` marks surfaces whose geometry
/// is a centered overlay popup; the service-log panel paints as a page body
/// band instead.
pub(crate) struct BatterySurface {
    pub(crate) name: &'static str,
    pub(crate) open: fn(&mut TuiApp) -> bool,
    pub(crate) popup: bool,
}

/// The full overlay-surface roster: settings, the six action menus, the
/// column menu, command palette, help, about, health, containers, process
/// properties, one armed confirmation per shared gate kind, the threshold
/// suggestions overlay and the service-log panel.
pub(crate) fn battery_surfaces() -> Vec<BatterySurface> {
    vec![
        BatterySurface {
            name: "settings",
            open: open_settings,
            popup: true,
        },
        BatterySurface {
            name: "about",
            open: open_about,
            popup: true,
        },
        BatterySurface {
            name: "health",
            open: open_health,
            popup: true,
        },
        BatterySurface {
            name: "containers",
            open: open_containers,
            popup: true,
        },
        BatterySurface {
            name: "help",
            open: open_help,
            popup: true,
        },
        BatterySurface {
            name: "command palette",
            open: open_command_palette,
            popup: true,
        },
        BatterySurface {
            name: "threshold suggestions",
            open: open_suggestions,
            popup: true,
        },
        BatterySurface {
            name: "process menu",
            open: open_process_menu,
            popup: true,
        },
        BatterySurface {
            name: "batch menu",
            open: open_batch_menu_surface,
            popup: true,
        },
        BatterySurface {
            name: "column menu",
            open: open_column_menu,
            popup: true,
        },
        BatterySurface {
            name: "process properties",
            open: open_properties,
            popup: true,
        },
        BatterySurface {
            name: "service menu",
            open: open_service_menu,
            popup: true,
        },
        BatterySurface {
            name: "service log panel",
            open: open_service_log_panel,
            popup: false,
        },
        BatterySurface {
            name: "session menu",
            open: open_session_menu,
            popup: true,
        },
        BatterySurface {
            name: "startup menu",
            open: open_startup_menu,
            popup: true,
        },
        BatterySurface {
            name: "end-task confirmation",
            open: open_end_task_confirmation,
            popup: true,
        },
        BatterySurface {
            name: "service control confirmation",
            open: open_service_control_confirmation,
            popup: true,
        },
        BatterySurface {
            name: "batch confirmation",
            open: open_batch_confirmation,
            popup: true,
        },
        BatterySurface {
            name: "session control confirmation",
            open: open_session_control_confirmation,
            popup: true,
        },
        BatterySurface {
            name: "startup control confirmation",
            open: open_startup_control_confirmation,
            popup: true,
        },
    ]
}

/// The i18n key whose current value names the surface's modal title. The Zh
/// honesty test asserts the frame paints this resolved value (never a
/// hardcoded string, so catalog copy may evolve without breaking the test).
pub(crate) fn surface_title_key(surface: &str) -> Option<&'static str> {
    Some(match surface {
        "settings" => "chrome.settings",
        "about" => "about.title",
        "health" => "health.system_health_alerts",
        "containers" => "containers.title",
        "help" => "menu.keyboard_reference",
        "command palette" => "help.palette_type",
        "threshold suggestions" => "alerts.threshold_suggestions",
        "process menu" => "proc.actions",
        "batch menu" => "proc.batch_actions",
        "column menu" => "tui.columns_title",
        "process properties" => "prop.process_details",
        "service menu" => "svc.service_actions",
        "service log panel" => "svc.logs",
        "session menu" => "users.session_actions",
        "startup menu" => "startup.applications",
        "end-task confirmation" => "confirm.process_title",
        "service control confirmation" => "confirm.service_title",
        "batch confirmation" => "confirm.batch_title",
        "session control confirmation" => "confirm.session_title",
        "startup control confirmation" => "confirm.startup_title",
        _ => return None,
    })
}

fn go_to(app: &mut TuiApp, page: AppPage) {
    let _ = app.apply_action(AppAction::SelectPage(page));
}

fn mark_one_demo_process(app: &mut TuiApp) {
    app.shell.selected_rows.insert(
        taskmanager_shell::ProcessRowIdentity::from_parts(
            4242,
            taskmanager_test_support::fixture_start_token(4242),
        )
        .expect("non-zero parts"),
    );
}

fn open_settings(app: &mut TuiApp) -> bool {
    app.toggle_settings();
    app.settings_open()
}

fn open_about(app: &mut TuiApp) -> bool {
    app.toggle_about();
    app.about_open()
}

fn open_health(app: &mut TuiApp) -> bool {
    app.toggle_health();
    app.health_open()
}

fn open_containers(app: &mut TuiApp) -> bool {
    app.toggle_containers();
    app.containers_open()
}

fn open_help(app: &mut TuiApp) -> bool {
    app.toggle_help();
    app.shell.help_open()
}

fn open_command_palette(app: &mut TuiApp) -> bool {
    app.open_command_palette();
    true
}

fn open_suggestions(app: &mut TuiApp) -> bool {
    app.shell.toggle_suggestions();
    app.shell.suggestions_open()
}

fn open_process_menu(app: &mut TuiApp) -> bool {
    go_to(app, AppPage::Applications);
    app.open_process_menu()
}

fn open_batch_menu_surface(app: &mut TuiApp) -> bool {
    go_to(app, AppPage::Applications);
    mark_one_demo_process(app);
    app.open_batch_menu()
}

fn open_column_menu(app: &mut TuiApp) -> bool {
    go_to(app, AppPage::Applications);
    app.toggle_column_menu();
    true
}

fn open_properties(app: &mut TuiApp) -> bool {
    go_to(app, AppPage::Applications);
    app.open_process_properties()
}

fn open_service_menu(app: &mut TuiApp) -> bool {
    go_to(app, AppPage::Services);
    app.open_service_menu()
}

fn open_service_log_panel(app: &mut TuiApp) -> bool {
    go_to(app, AppPage::Services);
    app.shell.open_service_log().is_some()
}

fn open_session_menu(app: &mut TuiApp) -> bool {
    go_to(app, AppPage::Users);
    app.open_session_menu()
}

fn open_startup_menu(app: &mut TuiApp) -> bool {
    go_to(app, AppPage::Startup);
    app.open_startup_menu()
}

fn open_end_task_confirmation(app: &mut TuiApp) -> bool {
    go_to(app, AppPage::Applications);
    if !app.open_process_menu() {
        return false;
    }
    // Row 0 is the gated End-task row: selecting it arms the shared gate and
    // emits no platform effect.
    let _ = app.process_menu_select();
    app.shell.pending_confirmation().is_some()
}

fn open_service_control_confirmation(app: &mut TuiApp) -> bool {
    go_to(app, AppPage::Services);
    if !app.open_service_menu() {
        return false;
    }
    // Row 1 (Stop) is a gated control action.
    app.service_menu_move(1);
    app.service_menu_select();
    app.shell.pending_confirmation().is_some()
}

fn open_batch_confirmation(app: &mut TuiApp) -> bool {
    go_to(app, AppPage::Applications);
    mark_one_demo_process(app);
    if !app.open_batch_menu() {
        return false;
    }
    // Row 1 is the gated batch Kill.
    app.batch_menu_move(1);
    let _ = app.batch_menu_select();
    app.shell.pending_confirmation().is_some()
}

fn open_session_control_confirmation(app: &mut TuiApp) -> bool {
    go_to(app, AppPage::Users);
    if !app.open_session_menu() {
        return false;
    }
    app.session_menu_select();
    app.shell.pending_confirmation().is_some()
}

fn open_startup_control_confirmation(app: &mut TuiApp) -> bool {
    go_to(app, AppPage::Startup);
    if !app.open_startup_menu() {
        return false;
    }
    app.startup_menu_select();
    app.shell.pending_confirmation().is_some()
}
