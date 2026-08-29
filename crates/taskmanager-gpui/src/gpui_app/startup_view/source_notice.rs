//! Startup-specific explanation for degraded provider facets.

use taskmanager_application::i18n;
use taskmanager_core::core::source::{SourceOutcome, SourceStatus};

/// Explain the exact startup facet affected by a degraded source. The generic
/// list banner can only say "provider failed"; this page knows that the
/// systemd-blame source owns per-entry boot cost while the inventory source
/// owns the rows themselves.
pub(super) fn startup_source_detail(sources: &[SourceStatus]) -> Option<String> {
    let mut fields = Vec::new();
    let mut providers = Vec::new();
    let mut inventory_degraded = false;
    for source in sources {
        if !matches!(
            source.outcome,
            SourceOutcome::Partial(_) | SourceOutcome::Unavailable(_)
        ) {
            continue;
        }
        let provider = source.provider.as_str();
        let field = match provider {
            "linux.startup.systemd-blame" => i18n::t("startup.source_field_blame"),
            "linux.startup.xdg" => {
                inventory_degraded = true;
                i18n::t("startup.source_field_xdg")
            }
            "linux.startup.systemd-user" => {
                inventory_degraded = true;
                i18n::t("startup.source_field_systemd")
            }
            "linux.startup.openrc" => {
                inventory_degraded = true;
                i18n::t("startup.source_field_openrc")
            }
            "linux.startup.init-detection" => {
                inventory_degraded = true;
                i18n::t("startup.source_field_init")
            }
            _ => {
                inventory_degraded = true;
                i18n::t("startup.source_field_generic")
            }
        };
        fields.push(field);
        providers.push(provider);
    }
    if fields.is_empty() {
        return None;
    }
    Some(
        i18n::t("startup.source_missing_detail")
            .replace("{fields}", &fields.join("、"))
            .replace("{providers}", &providers.join(" · "))
            .replace(
                "{availability}",
                i18n::t(if inventory_degraded {
                    "startup.source_inventory_degraded"
                } else {
                    "startup.source_inventory_available"
                }),
            ),
    )
}
