//! Pure systemd/OpenRC parsing and description lookup helpers.

#[cfg(test)]
#[path = "../../../tests/headless/engine/services/parsing/proptests.rs"]
mod proptests;

use std::fs;

use super::target::{
    openrc_service_id, systemd_unit_id, valid_openrc_service_name, valid_systemd_unit_name,
};
use super::{ServiceDeps, ServiceItem, ServiceRelationKind, ServiceStatus};

/// Pure parser for one bounded `systemctl show <unit> -p <properties>` query
/// (the **key=value** form — i.e. WITHOUT `--value`). We pick the key=value form
/// over `--value` (which prints one bare value per line in property order)
/// because matching by key name is robust to systemctl re-ordering or omitting
/// empty properties, whereas the `--value` form is positional and fragile.
///
/// Each supported input line is `Key=value` where `value` is a space-separated
/// list of unit names (possibly empty, e.g. `Requires=`). Unknown properties
/// are ignored because arbitrary `show` properties are not necessarily service
/// relationships. Input is trimmed line-by-line, so leading/trailing whitespace
/// and stray blank lines don't corrupt the result. The pure half of
/// `ServiceManager::fetch_deps`, split out for filesystem-free testing.
///
/// Examples (see `tests` module below for the full matrix):
/// ```text
/// # use taskmanager_platform_linux::parse_systemctl_show_deps;
/// let out = "Requires=sysinit.target basic.target\n\
///            Wants=display-manager.service\n\
///            WantedBy=graphical.target\n\
///            After=network.target\n";
/// let d = parse_systemctl_show_deps(out);
/// let requires = d
///     .relation_targets(&taskmanager_core::ServiceRelationKind::Requires)
///     .map(taskmanager_core::ServiceId::as_str)
///     .collect::<Vec<_>>();
/// assert_eq!(
///     requires,
///     [
///         "linux.service.systemd:sysinit.target",
///         "linux.service.systemd:basic.target"
///     ]
/// );
/// ```
pub fn parse_systemctl_show_deps(output: &str) -> ServiceDeps {
    let mut deps = ServiceDeps::default();
    for line in output.lines() {
        let line = line.trim();
        let Some((property, value)) = line.split_once('=') else {
            continue;
        };
        let kind = match property {
            "Requires" => ServiceRelationKind::Requires,
            "Wants" => ServiceRelationKind::Wants,
            "Requisite" => ServiceRelationKind::Requisite,
            "BindsTo" => ServiceRelationKind::BindsTo,
            "PartOf" => ServiceRelationKind::PartOf,
            "Conflicts" => ServiceRelationKind::Conflicts,
            "Before" => ServiceRelationKind::Before,
            "After" => ServiceRelationKind::After,
            "WantedBy" => ServiceRelationKind::WantedBy,
            "RequiredBy" => ServiceRelationKind::RequiredBy,
            "UpheldBy" => ServiceRelationKind::UpheldBy,
            _ => continue,
        };
        let targets = value
            .split_whitespace()
            .filter(|target| valid_systemd_unit_name(target))
            .map(systemd_unit_id)
            .collect::<Vec<_>>();
        deps.replace_relation_targets(kind, targets);
    }
    deps
}

/// Parse `rc-status --servicelist` output into [`ServiceItem`]s. Each service
/// line is `<name> [ <state> ... ]` where `<state>` is `started`, `stopped`, or
/// `crashed`; newer OpenRC appends a duration (`[  started 00:00:02 (0) ]`) —
/// only the first bracketed token is read as the state. Lines without a
/// `[ ... ]` token are runlevel/section headers (`Runlevel: default`,
/// `Dynamic Runlevel: hotplugged`, blank lines) and are skipped.
///
/// `name` is the first whitespace token before the `[` (matching the systemd
/// parser's `parts[0]` convention — the service name is the line's first column
/// in canonical rc-status output). Runtime state maps via
/// [`ServiceStatus::from`]: started→Active, stopped→Inactive, crashed→Failed.
/// `description` is left empty for the caller to fill from the init.d script.
pub fn parse_openrc_status(output: &str) -> Vec<ServiceItem> {
    let mut services = Vec::new();
    for line in output.lines() {
        let line = line.trim_end();
        if line.trim().is_empty() {
            continue;
        }
        // Find the trailing `[ ... ]` block; lines without one are headers.
        let Some(open) = line.rfind('[') else {
            continue;
        };
        let after = &line[open..];
        let Some(close_rel) = after.find(']') else {
            continue;
        };
        let bracketed = &line[open + 1..open + close_rel];
        let state_token = bracketed
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_lowercase();
        if state_token.is_empty() {
            continue;
        }
        let before = line[..open].trim();
        let name = before.split_whitespace().next().unwrap_or("").to_string();
        if !valid_openrc_service_name(&name) {
            continue;
        }
        services.push(ServiceItem::from_inventory(
            openrc_service_id(&name),
            name,
            ServiceStatus::from(state_token.as_str()),
            "",
            "loaded",
            state_token,
            "unknown",
        ));
    }
    services
}

/// Parse `rc-update show` output into [`ServiceItem`]s. Each line is
/// `<name> | <runlevel> [<runlevel> ...]`; the same service may appear on more
/// than one line (one per runlevel), so entries are deduped by name with their
/// runlevels merged (space-joined) into `description`. `rc-update show` carries
/// no runtime state, so `status` is [`ServiceStatus::Unknown`].
pub fn parse_openrc_update(output: &str) -> Vec<ServiceItem> {
    let mut names: Vec<String> = Vec::new();
    let mut runlevels: Vec<String> = Vec::new(); // parallel to `names`

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((name_part, runlevel_part)) = line.split_once('|') else {
            continue;
        };
        let name = name_part.trim().to_string();
        if !valid_openrc_service_name(&name) {
            continue;
        }
        let runlevel = runlevel_part.trim().to_string();
        if let Some(pos) = names.iter().position(|n| n == &name) {
            // Merge unique runlevel tokens while preserving OpenRC's order.
            for candidate in runlevel.split_whitespace() {
                if !runlevels[pos]
                    .split_whitespace()
                    .any(|existing| existing == candidate)
                {
                    if !runlevels[pos].is_empty() {
                        runlevels[pos].push(' ');
                    }
                    runlevels[pos].push_str(candidate);
                }
            }
        } else {
            names.push(name);
            runlevels.push(runlevel);
        }
    }

    names
        .into_iter()
        .zip(runlevels)
        .map(|(name, runlevel)| {
            ServiceItem::from_inventory(
                openrc_service_id(&name),
                name,
                ServiceStatus::Unknown,
                runlevel,
                "loaded",
                "unknown",
                "unknown",
            )
        })
        .collect()
}

/// Best-effort description from an OpenRC init.d script: read
/// `/etc/init.d/<name>` and delegate to the pure [`parse_openrc_description`].
/// Mirrors the systemd `ServiceManager::extract_description` I/O wrapper.
pub(super) fn extract_openrc_description(name: &str) -> Option<String> {
    let path = format!("/etc/init.d/{}", name);
    fs::read_to_string(&path)
        .ok()
        .and_then(|content| parse_openrc_description(&content))
}

/// Pure parser for a systemd unit file's `Description=` directive: scan TEXT
/// line-by-line and return the first non-empty value. The pure half of
/// `ServiceManager::extract_description`, split out so it can be exercised
/// without touching the filesystem.
///
/// * Requires `=` immediately after `Description`, so `Description_foo=...`
///   (and any other `Description<suffix>=`) is rejected.
/// * Strips one matching pair of surrounding quotes (`"` or `'`).
/// * Returns `None` for an empty value (after trim + quote strip), letting the
///   caller fall back to its default.
#[cfg(feature = "test-support")]
pub fn parse_unit_description(text: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("Description") else {
            continue;
        };
        let Some(val) = rest.strip_prefix('=') else {
            continue;
        };
        let val = strip_matching_quotes(val.trim()).trim();
        if !val.is_empty() {
            return Some(val.to_string());
        }
    }
    None
}

/// Pure parser for an OpenRC init.d `description=` directive: scan TEXT
/// line-by-line and return the first non-empty value. The pure half of
/// [`extract_openrc_description`], split out for filesystem-free testing.
///
/// * Requires `=` right after `description` (optional whitespace around the
///   `=` is allowed, since init.d scripts commonly write `description = "..."`),
///   so `description_foo=...` is rejected.
/// * Strips one matching pair of surrounding quotes (`"` or `'`).
/// * Returns `None` for an empty value, letting the caller fall back.
pub fn parse_openrc_description(text: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("description") else {
            continue;
        };
        // Only match `description=` (reject `description_foo=` etc.); allow
        // optional whitespace around the `=`.
        let rest = rest.trim_start();
        let Some(val) = rest.strip_prefix('=') else {
            continue;
        };
        let val = strip_matching_quotes(val.trim()).trim();
        if !val.is_empty() {
            return Some(val.to_string());
        }
    }
    None
}

/// Strip one matching pair of surrounding ASCII quotes (`"` or `'`) from `s`:
/// `"foo"` → `foo`, `'bar'` → `bar`. Mismatched or unbalanced quotes are left
/// untouched. Shared by both description parsers so their quote handling stays
/// identical. Returns a sub-slice of `s` (no allocation).
fn strip_matching_quotes(s: &str) -> &str {
    let n = s.len();
    if n >= 2 {
        let bytes = s.as_bytes();
        let (first, last) = (bytes[0], bytes[n - 1]);
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            // Safe without `unsafe`: the stripped bytes are ASCII quote chars
            // (exactly 1 byte each), so the interior is guaranteed valid UTF-8.
            return &s[1..n - 1];
        }
    }
    s
}
