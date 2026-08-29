//! Static Processes-page hierarchy summary.
//!
//! Applications / Background / Uncategorized is the product's only process
//! projection. This active segment preserves the existing compact visual and
//! capture selector without carrying any runtime choice or reducer branch.

use gpui::{App, Div, Entity, InteractiveElement, ParentElement, Window, div};
use taskmanager_ui::primitives::segmented::{Segment, Segmented};

use crate::gpui_app::root::{Hover, RootView};
use taskmanager_application::i18n;
use taskmanager_theme::Theme;

pub(super) fn hierarchy_summary(
    theme: &Theme,
    hovered: Option<&Hover>,
    entity: &Entity<RootView>,
) -> Div {
    let palette = theme.palette();
    let id = "process-category-tree";
    let is_hovered = hovered == Some(&Hover::Static(id));
    let hover_entity = entity.clone();
    let summary = Segmented::new("proc-mode-switcher", palette).segment(
        Segment::new(
            id,
            i18n::t("proc.mode_category_tree"),
            move |_window: &mut Window, _cx: &mut App| {},
            move |hovered: &bool, _window: &mut Window, cx: &mut App| {
                hover_entity.update(cx, |view, cx| {
                    view.set_hover(
                        if *hovered {
                            Some(Hover::Static(id))
                        } else {
                            None
                        },
                        cx,
                    );
                });
            },
        )
        .active(true)
        .hovered(is_hovered),
    );
    div()
        .debug_selector(|| "tm-proc-mode-switcher".to_string())
        .child(summary)
}
