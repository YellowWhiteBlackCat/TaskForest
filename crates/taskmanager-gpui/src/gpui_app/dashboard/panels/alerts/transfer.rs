//! Clipboard adapter for alert-rule transfer.
//!
//! GPUI owns the clipboard interaction; parsing and merging remain pure domain
//! operations. No filesystem access or blocking worker runs on the UI thread.

use gpui::{ClipboardItem, Div, Entity, ParentElement, Styled, div};
use tracing::warn;

use crate::gpui_app::elements;
use crate::gpui_app::root::RootView;
use taskmanager_application::i18n;
use taskmanager_application::{AlertRuleImportMode, ManagedAlertRule, ManagedAlertRuleEdit};
use taskmanager_core::core::alerts::{
    AlertRuleConflictPolicy, AlertRuleTransferEntry, AlertRuleTransferError,
    export_alert_rules_json, import_alert_rules_json,
};
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;

fn transfer_entries(rules: &[ManagedAlertRule]) -> Vec<AlertRuleTransferEntry> {
    rules
        .iter()
        .map(|managed| AlertRuleTransferEntry::new(managed.rule.clone(), managed.enabled))
        .collect()
}

fn managed_rules(entries: Vec<AlertRuleTransferEntry>) -> Vec<ManagedAlertRule> {
    entries.into_iter().map(ManagedAlertRule::from).collect()
}

fn action_feedback(template: &'static str, action: &'static str) -> String {
    template
        .replace("{action}", action)
        .replace("{target}", i18n::t("alerts.manage"))
}

fn record_feedback(
    view: &mut RootView,
    action: &'static str,
    succeeded: bool,
    cx: &mut gpui::Context<RootView>,
) {
    let template = if succeeded {
        i18n::t("feedback.action_succeeded")
    } else {
        i18n::t("feedback.action_failed")
    };
    view.show_local_feedback(action_feedback(template, action), cx);
}

pub(super) fn render_transfer_actions(
    theme: &Theme,
    rules: &[ManagedAlertRule],
    entity: Entity<RootView>,
) -> Div {
    let export_rules = transfer_entries(rules);
    let export_entity = entity.clone();
    let import_entity = entity;

    div()
        .flex()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(tokens::SPACE_6)
        .child(elements::pill(
            theme,
            "alert-rules-export",
            i18n::t("common.export"),
            false,
            false,
            move |_window, cx| match export_alert_rules_json(&export_rules) {
                Ok(json) => {
                    cx.write_to_clipboard(ClipboardItem::new_string(json));
                    export_entity.update(cx, |view, cx| {
                        record_feedback(view, i18n::t("common.export"), true, cx);
                        cx.notify();
                    });
                }
                Err(error) => {
                    warn!(target: "taskmanager.alerts", %error, "alert-rule export rejected");
                    export_entity.update(cx, |view, cx| {
                        record_feedback(view, i18n::t("common.export"), false, cx);
                        cx.notify();
                    });
                }
            },
            |_, _, _| {},
        ))
        .child(elements::pill(
            theme,
            "alert-rules-import",
            i18n::t("common.import"),
            false,
            false,
            move |_window, cx| {
                let imported = cx
                    .read_from_clipboard()
                    .and_then(|item| item.text())
                    .ok_or_else(|| {
                        AlertRuleTransferError::InvalidJson("clipboard has no text".to_string())
                    })
                    .and_then(|json| import_alert_rules_json(&json));
                import_entity.update(cx, move |view, cx| {
                    let result = imported.and_then(|rules| {
                        view.edit_dashboard_alert_rules(ManagedAlertRuleEdit::Import {
                            rules: managed_rules(rules),
                            mode: AlertRuleImportMode::Merge(
                                AlertRuleConflictPolicy::ReplaceExisting,
                            ),
                        })
                        .map(|_| ())
                    });
                    match result {
                        Ok(()) => {
                            record_feedback(view, i18n::t("common.import"), true, cx);
                        }
                        Err(error) => {
                            warn!(target: "taskmanager.alerts", %error, "alert-rule import rejected");
                            record_feedback(view, i18n::t("common.import"), false, cx);
                        }
                    }
                    cx.notify();
                });
            },
            |_, _, _| {},
        ))
}

#[cfg(test)]
#[path = "../../../../../tests/gui/gpui_gpui_app_dashboard_panels_alerts_transfer_tests.rs"]
mod tests;
