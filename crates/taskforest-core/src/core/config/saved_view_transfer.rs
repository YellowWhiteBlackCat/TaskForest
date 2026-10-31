//! Stable, platform-neutral saved-view (process preset) clipboard transfer
//! contract.
//!
//! The Iced and GPUI dashboards exchange user-created process view presets
//! through the clipboard, and both used to carry byte-identical copies of the
//! protocol: the format/version literals, the size ceilings, the strict error
//! vocabulary, the pretty-printed document shape, and the collision-renaming
//! rules. This module is that one authority. Like the alert-rule transfer it
//! owns JSON conversion only — no filesystem, no clipboard, no UI thread — and
//! it never names a frontend preset type: a caller maps its local preset
//! through the neutral [`ProcessViewPresetConfig`] and keeps its own display
//! name, built-in, and hideable-column semantics on its side of the boundary.

use std::collections::HashSet;
use std::fmt;

use serde::{Deserialize, Serialize};

use super::ProcessViewPresetConfig;

/// Stable discriminator written into saved-view transfer documents.
pub const SAVED_VIEW_TRANSFER_FORMAT: &str = "taskmanager.saved-process-views";
/// Latest saved-view transfer document version supported by this build.
pub const SAVED_VIEW_TRANSFER_VERSION: u64 = 1;

const MAX_TRANSFER_BYTES: usize = 1_048_576;
/// Hard ceiling for one saved-view collection, enforced on imports and on
/// every export so a shared document can never balloon past it.
pub const MAX_TRANSFER_PRESETS: usize = 1_000;
/// Longest portable preset name, counted in characters rather than bytes so a
/// name survives the clipboard in every locale.
pub const MAX_PRESET_NAME_CHARS: usize = 80;

/// A strict saved-view transfer failure. Every variant is the reason a
/// frontend surfaces as "this clipboard cannot be imported" — the document is
/// rejected whole, never partially applied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SavedViewTransferError {
    TooLarge,
    InvalidDocument,
    UnsupportedFormat,
    UnsupportedVersion { found: u64 },
    TooManyPresets,
    InvalidPreset { index: usize },
    IdSpaceExhausted,
    NameSpaceExhausted,
}

impl fmt::Display for SavedViewTransferError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge => formatter.write_str("saved-view document is too large"),
            Self::InvalidDocument => formatter.write_str("invalid saved-view document"),
            Self::UnsupportedFormat => formatter.write_str("unsupported saved-view format"),
            Self::UnsupportedVersion { found } => {
                write!(formatter, "unsupported saved-view version: {found}")
            }
            Self::TooManyPresets => write!(formatter, "too many saved-view presets"),
            Self::InvalidPreset { index } => write!(formatter, "invalid saved view {index}"),
            Self::IdSpaceExhausted => formatter.write_str("the saved-view id space is exhausted"),
            Self::NameSpaceExhausted => {
                formatter.write_str("no free saved-view name variant exists")
            }
        }
    }
}

impl std::error::Error for SavedViewTransferError {}

/// The pretty-printed v1 skeleton. Private on purpose (mirrors the alert-rule
/// wire DTO): callers move presets through the typed export/import functions
/// instead of hand-building documents, so the published field order and the
/// schema check stay in one place.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedViewTransferDocumentV1 {
    format: String,
    version: u64,
    presets: Vec<ProcessViewPresetConfig>,
}

/// Serialize a complete, versioned saved-view document.
///
/// The pretty-printed field order (`format`, `version`, `presets`) and the
/// missing trailing newline are stable for version 1, so a document produced
/// here is byte-identical to the ones the Iced and GPUI copies wrote before
/// this module existed. Names are validated exactly like imports, including
/// the preset ceiling.
pub fn export_saved_views_document(
    presets: &[ProcessViewPresetConfig],
) -> Result<String, SavedViewTransferError> {
    if presets.len() > MAX_TRANSFER_PRESETS {
        return Err(SavedViewTransferError::TooManyPresets);
    }
    for (index, preset) in presets.iter().enumerate() {
        if !saved_view_name_is_portable(&preset.name) {
            return Err(SavedViewTransferError::InvalidPreset { index });
        }
    }
    serde_json::to_string_pretty(&SavedViewTransferDocumentV1 {
        format: SAVED_VIEW_TRANSFER_FORMAT.to_string(),
        version: SAVED_VIEW_TRANSFER_VERSION,
        presets: presets.to_vec(),
    })
    .map_err(|_| SavedViewTransferError::InvalidDocument)
}

/// Parse and strictly validate a versioned saved-view document.
///
/// Unknown fields, a foreign `format`, an unsupported `version`, an oversized
/// collection, and a non-portable preset name are all rejected before the
/// caller touches any live state. Column/sort/filter tokens stay opaque here:
/// the frontend vocabulary maps them after this returns.
pub fn import_saved_views_document(
    json: &str,
) -> Result<Vec<ProcessViewPresetConfig>, SavedViewTransferError> {
    let document = parse_document(json)?;
    let presets = document.presets;
    if presets.len() > MAX_TRANSFER_PRESETS {
        return Err(SavedViewTransferError::TooManyPresets);
    }
    for (index, preset) in presets.iter().enumerate() {
        if !saved_view_name_is_portable(&preset.name) {
            return Err(SavedViewTransferError::InvalidPreset { index });
        }
    }
    Ok(presets)
}

fn parse_document(json: &str) -> Result<SavedViewTransferDocumentV1, SavedViewTransferError> {
    if json.len() > MAX_TRANSFER_BYTES {
        return Err(SavedViewTransferError::TooLarge);
    }
    let document: SavedViewTransferDocumentV1 =
        serde_json::from_str(json).map_err(|_| SavedViewTransferError::InvalidDocument)?;
    if document.format != SAVED_VIEW_TRANSFER_FORMAT {
        return Err(SavedViewTransferError::UnsupportedFormat);
    }
    if document.version != SAVED_VIEW_TRANSFER_VERSION {
        return Err(SavedViewTransferError::UnsupportedVersion {
            found: document.version,
        });
    }
    Ok(document)
}

/// Whether one preset name can travel in a transfer document: non-empty,
/// trimmed, free of control characters, and within [`MAX_PRESET_NAME_CHARS`].
#[must_use]
pub fn saved_view_name_is_portable(name: &str) -> bool {
    !name.is_empty()
        && name.trim() == name
        && name.chars().count() <= MAX_PRESET_NAME_CHARS
        && !name.chars().any(char::is_control)
}

/// The first free `"{base} (2)"`, `"{base} (3)"`, … variant of `base`.
///
/// `base` is returned unchanged when it is not occupied. The stem is truncated
/// by the suffix length first, so a renamed name never exceeds the portable
/// name ceiling.
pub fn unique_saved_view_name(
    base: &str,
    occupied: &HashSet<String>,
) -> Result<String, SavedViewTransferError> {
    if !occupied.contains(base) {
        return Ok(base.to_string());
    }
    // There are `occupied.len()` names, so testing one more suffix than that
    // must find a free candidate (pigeonhole principle).
    for offset in 0..=occupied.len() {
        let index = u64::try_from(offset)
            .ok()
            .and_then(|offset| 2_u64.checked_add(offset))
            .ok_or(SavedViewTransferError::NameSpaceExhausted)?;
        let suffix = format!(" ({index})");
        let stem_len = MAX_PRESET_NAME_CHARS.saturating_sub(suffix.chars().count());
        let stem: String = base.chars().take(stem_len).collect();
        let candidate = format!("{stem}{suffix}");
        if !occupied.contains(&candidate) {
            return Ok(candidate);
        }
    }
    Err(SavedViewTransferError::NameSpaceExhausted)
}

/// Collision-free names for one import, in import order, plus how many of
/// them had to be renamed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedViewImportNames {
    pub names: Vec<String>,
    pub renamed: usize,
}

/// Resolve collision-free names for a whole import atomically.
///
/// Names are deduplicated against `occupied` AND against each other in import
/// order, so one document holding the same name twice still yields distinct
/// presets. A failure means no name was resolved: the caller must not apply a
/// partial import.
pub fn resolve_saved_view_import_names(
    occupied: &HashSet<String>,
    requested: Vec<String>,
) -> Result<SavedViewImportNames, SavedViewTransferError> {
    let mut names = HashSet::with_capacity(occupied.len());
    names.extend(occupied.iter().cloned());
    let mut resolved = Vec::with_capacity(requested.len());
    let mut renamed = 0;
    for requested in requested {
        let unique = unique_saved_view_name(&requested, &names)?;
        if unique != requested {
            renamed += 1;
        }
        names.insert(unique.clone());
        resolved.push(unique);
    }
    Ok(SavedViewImportNames {
        names: resolved,
        renamed,
    })
}

/// Fresh preset ids plus the id the next import should start from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedViewIdAllocation {
    pub ids: Vec<u64>,
    pub next_id: u64,
}

/// Allocate `count` unused ids starting at `next_id`, skipping the occupied
/// set so an imported preset can never collide with a live one. A wrap-around
/// id space fails closed instead of reusing an id.
pub fn allocate_saved_view_ids(
    occupied: &HashSet<u64>,
    next_id: u64,
    count: usize,
) -> Result<SavedViewIdAllocation, SavedViewTransferError> {
    let mut ids = Vec::with_capacity(count);
    let mut candidate = next_id;
    while ids.len() < count {
        if !occupied.contains(&candidate) {
            ids.push(candidate);
        }
        candidate = candidate
            .checked_add(1)
            .ok_or(SavedViewTransferError::IdSpaceExhausted)?;
    }
    Ok(SavedViewIdAllocation {
        ids,
        next_id: candidate,
    })
}

#[cfg(test)]
#[path = "../../../tests/headless/core_core_config_saved_view_transfer_tests.rs"]
mod tests;
