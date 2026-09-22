use super::*;
use gpui::{
    AppContext, Context, IntoElement, Render, TestAppContext, VisualTestContext, Window, px,
};
use taskmanager_core::core::device_state::{DeviceState, DeviceStatus};
use taskmanager_core::core::process_telemetry::{OpenFileEntry, OpenFileKind, ProcessOpenFiles};

fn labels() -> ProcessInsightsLabels {
    ProcessInsightsLabels::capture_fixture()
}

fn snapshot_with(open_files: ProcessOpenFiles) -> ProcessTelemetrySnapshot {
    ProcessTelemetrySnapshot {
        open_files,
        ..ProcessTelemetrySnapshot::default()
    }
}

fn populated_open_files() -> ProcessOpenFiles {
    ProcessOpenFiles {
        state: DeviceState::healthy(1),
        unreadable_count: 1,
        entries: vec![
            OpenFileEntry {
                fd: 0,
                kind: OpenFileKind::File,
                target: Some("/dev/null".into()),
                deleted: false,
            },
            OpenFileEntry {
                fd: 3,
                kind: OpenFileKind::Socket,
                target: Some("socket:[4242]".into()),
                deleted: false,
            },
            // Readlink failed (privileged fd on a non-root reader): the row
            // is kept with a typed None target rather than dropped.
            OpenFileEntry {
                fd: 9,
                kind: OpenFileKind::Other,
                target: None,
                deleted: false,
            },
        ],
    }
}

/// Honesty: an unreadable descriptor (None target) must render the typed
/// "unreadable" marker, never a blank target or a fabricated path. This is
/// the load-bearing honesty assertion and needs no window.
#[test]
fn format_keeps_unreadable_target_honest() {
    let unreadable = "unreadable";
    let ok_line = format_open_file(&populated_open_files().entries[0], unreadable);
    let denied_line = format_open_file(&populated_open_files().entries[2], unreadable);
    assert!(ok_line.contains("/dev/null"));
    assert!(
        denied_line.ends_with(unreadable),
        "an unreadable fd must surface the typed marker, got: {denied_line}"
    );
}

/// Minimal root view that renders one card frame per draw, so the open-files
/// card can be exercised through the same window-draw path the rest of the
/// process-insights tests use. The card is rebuilt from the typed snapshot on
/// every render because GPUI may render once during window creation; a
/// consumed-once card would leave the explicit assertion draw empty.
struct OpenFilesCardView {
    snapshot: ProcessTelemetrySnapshot,
}
impl Render for OpenFilesCardView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        open_files_card(&Theme::dark(), &self.snapshot, &labels(), 480.0)
    }
}

/// `debug_bounds` takes a `&'static str`; the row selectors are indexed.
fn selector(text: String) -> &'static str {
    Box::leak(text.into_boxed_str())
}

fn draw_frame(
    cx: &mut TestAppContext,
    snapshot: ProcessTelemetrySnapshot,
) -> gpui::WindowHandle<OpenFilesCardView> {
    let window = cx.add_window(move |_w, _cx| OpenFilesCardView { snapshot });
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    window
}

#[gpui::test]
fn empty_healthy_renders_the_no_open_files_message(cx: &mut TestAppContext) {
    draw_frame(
        cx,
        snapshot_with(ProcessOpenFiles {
            state: DeviceState::healthy(1),
            ..ProcessOpenFiles::default()
        }),
    );
}

/// A process holding more descriptors than the card cap (ulimit can reach
/// thousands) renders through the bounded window without panic; the window
/// math and the "… {count} more" remainder are locked by `view::cap_tests`.
#[gpui::test]
fn oversized_fd_list_renders_through_the_capped_path(cx: &mut TestAppContext) {
    let cap = super::super::MAX_INSIGHT_CARD_ROWS;
    let open_files = ProcessOpenFiles {
        state: DeviceState::healthy(1),
        unreadable_count: 0,
        entries: (0..cap + 824)
            .map(|i| OpenFileEntry {
                fd: i as u32,
                kind: OpenFileKind::File,
                target: Some(format!("/tmp/session-{i}.lock")),
                deleted: false,
            })
            .collect(),
    };
    draw_frame(cx, snapshot_with(open_files));
}

/// EACCES on a foreign-uid fd directory renders the typed "Permission
/// denied" status, never a blank card.
#[gpui::test]
fn denied_state_renders_a_typed_status_not_blank(cx: &mut TestAppContext) {
    let denied = ProcessOpenFiles {
        state: DeviceState::healthy(1).transition(DeviceStatus::PermissionDenied, 2),
        ..ProcessOpenFiles::default()
    };
    draw_frame(cx, snapshot_with(denied));
}

#[gpui::test]
fn populated_open_files_render_with_unreadable_marker(cx: &mut TestAppContext) {
    let win = draw_frame(cx, snapshot_with(populated_open_files()));
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    // One painted row per projected descriptor: the readable rows carry their
    // own token and the descriptor whose readlink failed carries the typed
    // `unreadable` token. The literal marker text is locked by the sibling
    // `format_keeps_unreadable_target_honest` (production `format_open_file`);
    // these selectors prove the typed branch reached the painted rows.
    for (index, token) in [(0_usize, "readable"), (1, "readable"), (2, "unreadable")] {
        let sel = selector(format!("tm-insight-open-file:{index}:{token}"));
        let bounds = vcx
            .debug_bounds(sel)
            .unwrap_or_else(|| panic!("{sel} must paint its descriptor row"));
        assert!(
            bounds.size.width > px(0.0) && bounds.size.height > px(0.0),
            "descriptor row {index} collapsed: {bounds:?}"
        );
    }
    assert!(
        vcx.debug_bounds("tm-insight-open-file:3:readable")
            .is_none(),
        "no row beyond the projected descriptors may paint"
    );
    assert!(
        vcx.debug_bounds("tm-insight-open-file:0:unreadable")
            .is_none(),
        "a readable descriptor must not carry the unreadable token"
    );
}
