//! Declarative properties chrome around one bounded complete inspection body.

use super::{
    ProcessPropertiesAction, ProcessPropertiesBody, ProcessPropertiesDismissButton,
    ProcessPropertiesFacet, ProcessPropertiesFooter, ProcessPropertiesOverlay,
    ProcessPropertiesPanel, ProcessPropertiesScrollHint, ProcessPropertiesSection,
    ProcessPropertiesTab, ProcessPropertiesView, on_authorize, on_facet,
    on_properties_dismiss_activated, on_refresh, on_tab,
};
use crate::palette::{UiPalette, space_4, space_8, space_12};
use crate::widgets::chart::{CurveMeasurement, paint_curve_at_size};
use crate::widgets::controls::{ControlTone, ControlVisual, detail_row_scene};
use crate::window::{Role, TextRole};
use bevy::color::Color;
use bevy::ecs::{component::Component, entity::Entity, hierarchy::Children, world::World};
use bevy::scene::{Scene, bsn, on};
use bevy::text::{LineBreak, TextLayout};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, ComputedNode, FlexDirection, FlexWrap,
    JustifyContent, Node, Overflow, PositionType, ScrollPosition, UiRect, percent, px,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::{Button, ScrollArea};
use std::sync::Arc;
use taskmanager_application::{
    ProcessInsightFacet, i18n::t, process_details_vm::ProcessDetailsField,
};
use taskmanager_shell::presentation::process_insight_facet_label;

#[derive(Component, Clone)]
pub(crate) struct ProcessPropertiesCurve {
    pub(crate) metric: ProcessDetailsField,
    pub(crate) samples: Arc<[f32]>,
    ceiling: f32,
    color: Color,
}
impl Default for ProcessPropertiesCurve {
    fn default() -> Self {
        Self {
            metric: ProcessDetailsField::Cpu,
            samples: Arc::from([]),
            ceiling: 100.0,
            color: Color::WHITE,
        }
    }
}
fn text_scene(value: String) -> Box<dyn Scene> {
    Box::new(
        bsn! { Text(value) TextRole(Role::Body) TextLayout { linebreak: LineBreak::WordBoundary }
        Node { width: percent(100), min_width: px(0.0) } },
    )
}
pub(super) fn properties_modal_scene(
    view: &ProcessPropertiesView,
    palette: &UiPalette,
    scroll: ScrollPosition,
) -> impl Scene + use<> {
    let title = format!("{} · {}", t("dialog.properties"), view.target.name);
    let subtitle = format!("PID: {}", view.target.pid);
    let tabs: Vec<Box<dyn Scene>> = ProcessPropertiesSection::ALL.into_iter().map(|section| {
        Box::new(bsn! { Node { min_height: px(palette.control_height_px), padding: UiRect::horizontal(px(space_8())) }
            Button ProcessPropertiesTab(section) ControlVisual(ControlTone::Surface, {section == view.section}) on(on_tab)
            Children [ Text({section.label().to_owned()}) TextRole(Role::Body) ] }) as Box<dyn Scene>
    }).collect();
    let facets: Vec<Box<dyn Scene>> = if view.section == ProcessPropertiesSection::Insights {
        ProcessInsightFacet::ALL.into_iter().map(|facet| Box::new(bsn! { Node { min_height: px(palette.control_height_px), padding: UiRect::horizontal(px(space_8())) }
            Button ProcessPropertiesFacet(facet) ControlVisual(ControlTone::Surface, {facet == view.facet}) on(on_facet)
            Children [ Text({process_insight_facet_label(facet).to_owned()}) TextRole(Role::Caption) ] }) as Box<dyn Scene>).collect()
    } else {
        Vec::new()
    };
    let mut content: Vec<Box<dyn Scene>> = view
        .rows
        .iter()
        .map(|(label, value)| {
            Box::new(detail_row_scene(
                label.clone(),
                text_scene(value.clone()),
                palette,
            )) as Box<dyn Scene>
        })
        .collect();
    content.insert(
        0,
        Box::new(
            bsn! { Text({t("proc_insights.scroll_hint").to_owned()}) TextRole(Role::Caption)
            ProcessPropertiesScrollHint TextLayout { linebreak: LineBreak::WordBoundary }
            Node { width: percent(100), min_height: px(14.0), flex_shrink: 0.0 } },
        ),
    );
    if !view.curves.is_empty() {
        content.push(Box::new(
            bsn! { Text({t("prop.recent_samples").to_owned()}) TextRole(Role::Caption) },
        ));
    }
    for curve in &view.curves {
        let color = match curve.metric {
            ProcessDetailsField::Cpu => palette.cpu,
            ProcessDetailsField::Memory => palette.memory,
            _ => palette.disk,
        };
        let caption = format!(
            "{} {} · {} {}",
            t("dashboard.latest"),
            curve.current,
            t("dashboard.peak"),
            curve.peak
        );
        let mut body: Vec<Box<dyn Scene>> = vec![
            Box::new(bsn! { Text({curve.label.clone()}) TextRole(Role::Body) }),
            Box::new(
                bsn! { Text(caption) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::WordBoundary } Node { width: percent(100), min_width: px(0.0) } },
            ),
        ];
        if curve
            .samples
            .iter()
            .filter(|value| value.is_finite())
            .count()
            >= 2
        {
            body.push(Box::new(bsn! { Node { width: percent(100), height: px(56.0), flex_shrink: 0.0, overflow: Overflow::clip() }
                ProcessPropertiesCurve { metric: {curve.metric}, samples: {curve.samples.clone()}, ceiling: {curve.ceiling}, color }
                CurveMeasurement::default() }));
        } else {
            body.push(text_scene(t("proc_insights.collecting").to_owned()));
        }
        content.push(Box::new(bsn! { Node { width: percent(100), min_width: px(0.0), flex_direction: FlexDirection::Column,
            row_gap: px(space_4()), padding: UiRect::all(px(space_8())), flex_shrink: 0.0 }
            BackgroundColor({palette.content_bg}) Children [{body}] }));
    }
    let mut actions: Vec<Box<dyn Scene>> = Vec::new();
    if view.section == ProcessPropertiesSection::Insights && view.live {
        actions.push(Box::new(bsn! { Node { min_height: px(palette.control_height_px), padding: UiRect::horizontal(px(space_8())) }
            Button ProcessPropertiesAction::Refresh ControlVisual(ControlTone::Surface, false) on(on_refresh)
            Children [ Text({t("common.refresh").to_owned()}) TextRole(Role::Body) ] }));
    }
    if view.authorize_network {
        actions.push(Box::new(bsn! { Node { min_height: px(palette.control_height_px), padding: UiRect::horizontal(px(space_8())) }
            Button ProcessPropertiesAction::AuthorizeNetwork ControlVisual(ControlTone::Surface, true) on(on_authorize)
            Children [ Text({t("proc_insights.enable_network_capture").to_owned()}) TextRole(Role::Body)
                TextLayout { linebreak: LineBreak::WordBoundary } Node { min_width: px(0.0) } ] }));
    }
    actions.push(Box::new(bsn! { Node { min_height: px(palette.control_height_px), padding: UiRect::horizontal(px(space_8())) }
        Button ProcessPropertiesDismissButton ControlVisual(ControlTone::Surface, true) on(on_properties_dismiss_activated)
        Children [ Text({t("common.close").to_owned()}) TextRole(Role::Body) ] }));
    bsn! {
        Node { width: percent(100), height: percent(100), position_type: PositionType::Absolute,
            justify_content: JustifyContent::Center, align_items: AlignItems::Center }
        BackgroundColor({palette.scrim}) ProcessPropertiesOverlay
        Children [
            Node { width: percent(92), max_width: px(760.0), height: percent(92), min_height: px(0.0), min_width: px(0.0),
                flex_direction: FlexDirection::Column, row_gap: px(space_8()), padding: UiRect::all(px(space_12())),
                border_radius: BorderRadius::all(px(palette.panel_radius_px)) }
            BackgroundColor({palette.panel_fill}) ProcessPropertiesPanel
            Children [
                Text(title) TextRole(Role::Heading) TextLayout { linebreak: LineBreak::WordBoundary }
                    Node { width: percent(100), min_width: px(0.0), max_height: px(48.0), flex_shrink: 0.0, overflow: Overflow::clip() } --
                Text(subtitle) TextRole(Role::Caption) Node { flex_shrink: 0.0 } --
                Node { width: percent(100), flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap,
                    row_gap: px(space_4()), column_gap: px(space_4()), flex_shrink: 0.0 } Children [{tabs}] --
                Node { width: percent(100), flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap,
                    row_gap: px(space_4()), column_gap: px(space_4()), flex_shrink: 0.0 } Children [{facets}] --
                Node { width: percent(100), min_width: px(0.0), flex_grow: 1.0, flex_basis: px(0.0), min_height: px(0.0),
                    flex_direction: FlexDirection::Column, row_gap: px(space_4()), overflow: Overflow::scroll_y(),
                    padding: UiRect::bottom(px(space_8())) }
                ScrollArea ScrollPosition({scroll.0}) ProcessPropertiesBody Children [{content}] --
                Node { width: percent(100), flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap,
                    row_gap: px(space_4()), column_gap: px(space_8()), flex_shrink: 0.0 }
                ProcessPropertiesFooter Children [{actions}]
            ]
        ]
    }
}
pub(super) fn paint_curves(world: &mut World) {
    let pending = world
        .query::<(
            Entity,
            &ComputedNode,
            &ProcessPropertiesCurve,
            &CurveMeasurement,
        )>()
        .iter(world)
        .filter_map(|(entity, node, curve, last)| {
            let size = node.size() * node.inverse_scale_factor();
            (size.x > 0.0 && size.y > 0.0 && last.0 != Some((size.x, size.y)))
                .then(|| (entity, size, curve.clone()))
        })
        .collect::<Vec<_>>();
    for (entity, size, curve) in pending {
        if paint_curve_at_size(
            world,
            entity,
            size,
            &curve.samples,
            curve.ceiling,
            curve.color,
        ) && let Some(mut measured) = world.get_mut::<CurveMeasurement>(entity)
        {
            measured.0 = Some((size.x, size.y));
        }
    }
}
