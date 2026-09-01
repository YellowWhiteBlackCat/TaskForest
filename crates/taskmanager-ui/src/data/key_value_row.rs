//! Shared label/value row geometry.

use gpui::{
    App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce, SharedString,
    Styled, Window, div, px,
};
use taskmanager_theme::{Color, Length, Palette, tokens};

use crate::primitives::selectable_text::SelectableText;

/// A two-column label/value row for stat and specification panels.
#[derive(IntoElement)]
pub struct KeyValueRow {
    label: String,
    value: String,
    palette: Palette,
    label_width: Option<Length>,
    value_color: Option<Color>,
    value_align_right: bool,
    selectable_value_id: Option<ElementId>,
    value_debug_selector: Option<SharedString>,
}

impl KeyValueRow {
    /// Build a row from display-ready strings.
    pub fn new(label: impl Into<String>, value: impl Into<String>, palette: Palette) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            palette,
            label_width: None,
            value_color: None,
            value_align_right: true,
            selectable_value_id: None,
            value_debug_selector: None,
        }
    }

    /// Reserve a stable label width when a panel uses aligned values.
    #[must_use]
    pub fn label_width(mut self, width: Length) -> Self {
        self.label_width = Some(width);
        self
    }

    /// Override the value color for missing/secondary observations.
    #[must_use]
    pub fn value_color(mut self, color: Color) -> Self {
        self.value_color = Some(color);
        self
    }

    /// Keep long values in normal reading order instead of right-aligning
    /// them; useful for process-insight detail rows.
    #[must_use]
    pub fn value_align_right(mut self, align: bool) -> Self {
        self.value_align_right = align;
        self
    }

    /// Allow pointer selection and Ctrl/Cmd+C for the value column.
    ///
    /// The caller supplies a stable semantic ID because labels such as
    /// “Status” may appear in several cards on the same page.
    #[must_use]
    pub fn selectable_value(mut self, id: impl Into<ElementId>) -> Self {
        self.selectable_value_id = Some(id.into());
        self
    }

    /// Stable geometry selector for the actual selectable value surface.
    /// Page tests should prefer this semantic hook over localized label text.
    #[must_use]
    pub fn value_debug_selector(mut self, selector: impl Into<SharedString>) -> Self {
        self.value_debug_selector = Some(selector.into());
        self
    }

    /// Render the row.
    #[must_use]
    pub fn render(self) -> gpui::Div {
        let row_selector = format!("tm-key-value-row:{}", self.label);
        let value_selector = format!("tm-key-value-value:{}", self.label);
        let selectable_selector = self.value_debug_selector.unwrap_or_else(|| {
            SharedString::from(format!("tm-key-value-selectable:{}", self.label))
        });
        let mut label = div()
            .min_w(px(0.0))
            .truncate()
            .text_size(crate::theme_binding::font_size(tokens::FONT_12))
            .text_color(crate::theme_binding::hsla(self.palette.fg_muted))
            .child(self.label);
        let mut value = div()
            .min_w(px(0.0))
            .text_size(crate::theme_binding::font_size(tokens::FONT_12))
            .text_color(crate::theme_binding::hsla(
                self.value_color.unwrap_or(self.palette.fg),
            ))
            .debug_selector(move || value_selector.clone());
        value = match self.selectable_value_id {
            Some(id) if self.value_align_right => value.child(
                SelectableText::new(id, self.value, self.palette)
                    .single_line()
                    .debug_selector(selectable_selector),
            ),
            Some(id) => value.child(
                SelectableText::new(id, self.value, self.palette)
                    .debug_selector(selectable_selector),
            ),
            None => value.child(self.value),
        };
        if let Some(width) = self.label_width {
            // Property/detail rows reserve a stable label column and let the
            // value consume the remaining width of the bounded panel.
            label = label.w(crate::theme_binding::length(width)).flex_shrink_0();
            value = value.flex_1();
        } else {
            // Stat/spec rows have no external label token. Let the label own
            // the elastic side and keep the short readout intrinsic; making
            // both children flex-grow leaves a narrow value at zero width in
            // GPUI's first flex measurement, which then wraps every
            // character. Shrinkable-with-full-max bounds an observation
            // longer than the row (a serial number, a trend line) to the
            // row itself — minus the column gap — so it truncates at its own
            // end instead of pushing past the panel and clipping mid-string
            // at the window edge.
            label = label.flex_1().truncate();
            value = value.flex_shrink().max_w_full();
        }
        if self.value_align_right {
            // A value column is a single-line readout. Without the shared
            // truncate contract, a narrow details panel wraps values such as
            // `51 °C` and `0.43 GHz` at their internal space, making the right
            // column appear vertically misaligned even though its flex bounds
            // are correct.
            // `overflow_hidden` is explicit as well: a selectable text child
            // can retain its intrinsic paint width even after the parent has
            // flex-shrunk. The rail must clip/truncate inside its value slot,
            // never at the outer window edge.
            value = value.truncate().overflow_hidden().text_right();
        }
        div()
            .flex()
            .flex_row()
            .items_start()
            .justify_between()
            .gap(crate::theme_binding::definite_length(tokens::SPACE_12))
            .w_full()
            .min_w(px(0.0))
            .debug_selector(move || row_selector.clone())
            .child(label)
            .child(value)
    }
}

impl RenderOnce for KeyValueRow {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        self.render()
    }
}
