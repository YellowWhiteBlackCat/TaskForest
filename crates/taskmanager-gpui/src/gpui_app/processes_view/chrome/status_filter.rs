//! Process-table status-filter pills (design-debt #1/#7: chrome.rs line split).

use gpui::{App, Div, Entity, InteractiveElement, ParentElement, Window, div};

use crate::gpui_app::root::{Hover, RootView};
use crate::gpui_app::theme::Theme;
use taskmanager_shell::ProcessStatusFilter;

pub fn status_filter_row(
    theme: &Theme,
    filter: ProcessStatusFilter,
    hovered: Option<&Hover>,
    entity: &Entity<RootView>,
) -> Div {
    use taskmanager_ui::primitives::segmented::{Segment, Segmented};
    let palette = theme.palette();
    let mut ctrl = Segmented::new("proc-status-filter", palette);
    for &this in ProcessStatusFilter::ALL.iter() {
        // The shell owns the control identity (`key()`, the same "all" id
        // family the iced/TUI segmented controls use) and the localized
        // label — the pill row is a pure projection of the shell filter.
        let id_str = this.key();
        let active = this == filter;
        let is_hov = hovered == Some(&Hover::Static(id_str));
        let ent_c = entity.clone();
        let ent_h = entity.clone();
        let label = this.label();
        ctrl = ctrl.segment(
            Segment::new(
                id_str,
                label,
                move |_win: &mut Window, cx: &mut App| {
                    ent_c.update(cx, |v, cx| {
                        v.set_process_status_filter(this);
                        cx.notify();
                    });
                },
                move |is_hov: &bool, _win: &mut Window, cx: &mut App| {
                    ent_h.update(cx, |v, cx| {
                        v.set_hover(
                            if *is_hov {
                                Some(Hover::Static(id_str))
                            } else {
                                None
                            },
                            cx,
                        );
                    });
                },
            )
            .active(active)
            .hovered(is_hov),
        );
    }
    div()
        .debug_selector(|| "tm-proc-status-filter".to_string())
        .child(ctrl)
}
