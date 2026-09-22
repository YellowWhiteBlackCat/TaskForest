//! The P4 feature-level evidence table and its G2 closure (P5 M3.4).
//!
//! This submodule is the ONE authority for "which `(feature, frontend)` pair
//! carries a hand-declared behaviour anchor". The committed table is embedded
//! at compile time, so a stale or missing file cannot silently pass, and it is
//! declaration data, never a source scan: the resolver
//! (`scripts/parity/resolve_frontend_evidence.py`) proves every anchored row is
//! a real `cargo nextest list` id.

use taskmanager_platform_contract::{PlatformAxis, PlatformCapabilitySurface, PlatformSource};

use super::super::FeatureId;
use super::super::NO_EVIDENCE;
use super::super::platform_binding::PlatformBinding;
use super::evidence_is_usable;
use crate::keybindings::FrontendShape;

// -- P4 feature-level evidence closure (G2) ---------------------------------
//
// The committed table is the ONE authority for "which (feature, frontend) pair
// carries a hand-declared behaviour anchor". It is declaration data, never a
// source scan: the resolver proves each anchor is a real `cargo nextest list`
// id. The closure below folds the table against the layer-B surface, so an
// anchor attaches only where the static source commitment is complete.

/// The committed feature-level evidence table, embedded at compile time so a
/// stale or missing file cannot silently pass.
const FEATURE_EVIDENCE_TABLE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../scripts/parity/feature_evidence.tsv"
));

/// The exact column header of the committed feature-evidence table.
const FEATURE_EVIDENCE_HEADER: &str = "feature_id\tfrontend\ttest_id\tstatus\tnote";

/// The "no value" marker the committed table uses for the unused column of a
/// row (`test_id` on a pending row, `note` on an anchored row).
const FEATURE_EVIDENCE_DASH: &str = "-";

/// Whether a committed feature-evidence row carries a resolvable anchor or is
/// an explicit, reasoned gap.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FeatureEvidenceStatus {
    /// A hand-declared nextest test id backs the cell.
    Anchored,
    /// The `(feature, frontend)` pair was surveyed and has no discoverable
    /// behaviour test yet; the note states the gap.
    Pending,
}

impl FeatureEvidenceStatus {
    /// Stable machine id used in reports and diagnostics.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Anchored => "anchored",
            Self::Pending => "pending",
        }
    }

    /// The row status for one committed `status` token, if it is known.
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "anchored" => Some(Self::Anchored),
            "pending" => Some(Self::Pending),
            _ => None,
        }
    }
}

/// One parsed row of the committed feature-evidence table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeatureEvidenceRow {
    /// The feature the anchor proves.
    pub feature: FeatureId,
    /// The frontend that owns the anchor test.
    pub frontend: FrontendShape,
    /// `Anchored` or `Pending`.
    pub status: FeatureEvidenceStatus,
    /// The nextest test id of an anchored row; empty for a pending row.
    pub anchor: &'static str,
    /// The gap reason of a pending row; empty for an anchored row.
    pub note: &'static str,
    /// 1-based line in the committed table, for diagnostics.
    pub line: usize,
}

impl FeatureEvidenceRow {
    /// Whether the row carries a hand-declared anchor.
    #[must_use]
    pub const fn is_anchored(&self) -> bool {
        matches!(self.status, FeatureEvidenceStatus::Anchored)
    }
}

/// A structural defect in a feature-evidence table.
///
/// The committed table is data whose contract is its own text, so a defect is
/// reported as a finding instead of being skipped; an empty list is the only
/// passing state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FeatureEvidenceFinding {
    /// The first non-comment line is not the declared column header.
    MalformedHeader { line: usize },
    /// A data row does not carry exactly five columns.
    MalformedRow { line: usize },
    /// `feature_id` is not a registered [`FeatureId`].
    UnknownFeature { line: usize, raw: &'static str },
    /// `frontend` is not a registered [`FrontendShape`].
    UnknownFrontend { line: usize, raw: &'static str },
    /// `status` is neither `anchored` nor `pending`.
    UnknownStatus { line: usize, raw: &'static str },
    /// The same `(feature, frontend)` cell is declared twice.
    DuplicateCell {
        feature: FeatureId,
        frontend: FrontendShape,
        line: usize,
    },
    /// An anchored row carries an empty or placeholder anchor.
    UnusableAnchor {
        feature: FeatureId,
        frontend: FrontendShape,
        line: usize,
    },
    /// An anchored row carries a note instead of the `-` marker.
    AnnotatedAnchor {
        feature: FeatureId,
        frontend: FrontendShape,
        line: usize,
    },
    /// A pending row names a test id instead of the `-` marker.
    PendingWithAnchor {
        feature: FeatureId,
        frontend: FrontendShape,
        line: usize,
    },
    /// A pending row carries no gap reason.
    MissingPendingNote {
        feature: FeatureId,
        frontend: FrontendShape,
        line: usize,
    },
    /// An anchored row names a feature whose binding can never deliver
    /// (`NotApplicable`/`NotInVocabulary`), so no cell could carry the anchor.
    AnchorOnNonDeliveryBinding {
        feature: FeatureId,
        frontend: FrontendShape,
        line: usize,
    },
    /// The table declares no data row at all.
    EmptyTable,
}

/// The parsed committed feature-level evidence table.
///
/// The rows are a bounded first batch plus explicit `pending` gaps; absence
/// from this table is "not surveyed yet" and never a delivery claim, because a
/// `Ready` cell still requires a committed anchor.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FeatureEvidenceTable {
    rows: Vec<FeatureEvidenceRow>,
    findings: Vec<FeatureEvidenceFinding>,
}

impl FeatureEvidenceTable {
    /// Parse the committed table.
    #[must_use]
    pub fn committed() -> Self {
        Self::parse(FEATURE_EVIDENCE_TABLE)
    }

    /// Parse one evidence table.
    ///
    /// The input is `&'static str` because every production use embeds the
    /// committed file with `include_str!`; the rows borrow their slices from
    /// that one committed address instead of copying them. A malformed row is
    /// reported as a finding and skipped, never panicking.
    #[must_use]
    pub fn parse(table: &'static str) -> Self {
        let mut rows: Vec<FeatureEvidenceRow> = Vec::new();
        let mut findings: Vec<FeatureEvidenceFinding> = Vec::new();
        let mut header_seen = false;
        for (index, raw) in table.lines().enumerate() {
            let line = index + 1;
            let trimmed = raw.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if !header_seen {
                header_seen = true;
                if trimmed != FEATURE_EVIDENCE_HEADER {
                    findings.push(FeatureEvidenceFinding::MalformedHeader { line });
                }
                continue;
            }
            let fields: Vec<&'static str> = trimmed.split('\t').map(str::trim).collect();
            if fields.len() != 5 {
                findings.push(FeatureEvidenceFinding::MalformedRow { line });
                continue;
            }
            let Some(feature) = feature_by_id(fields[0]) else {
                findings.push(FeatureEvidenceFinding::UnknownFeature {
                    line,
                    raw: fields[0],
                });
                continue;
            };
            let Some(frontend) = frontend_by_name(fields[1]) else {
                findings.push(FeatureEvidenceFinding::UnknownFrontend {
                    line,
                    raw: fields[1],
                });
                continue;
            };
            let Some(status) = FeatureEvidenceStatus::from_id(fields[3]) else {
                findings.push(FeatureEvidenceFinding::UnknownStatus {
                    line,
                    raw: fields[3],
                });
                continue;
            };
            let anchor = fields[2];
            let note = fields[4];
            let mut defect = None;
            match status {
                FeatureEvidenceStatus::Anchored => {
                    if !evidence_is_usable(anchor) {
                        defect = Some(FeatureEvidenceFinding::UnusableAnchor {
                            feature,
                            frontend,
                            line,
                        });
                    } else if note != FEATURE_EVIDENCE_DASH {
                        defect = Some(FeatureEvidenceFinding::AnnotatedAnchor {
                            feature,
                            frontend,
                            line,
                        });
                    } else if !matches!(
                        feature.platform_binding(),
                        PlatformBinding::Requires(capabilities) if !capabilities.is_empty()
                    ) {
                        defect = Some(FeatureEvidenceFinding::AnchorOnNonDeliveryBinding {
                            feature,
                            frontend,
                            line,
                        });
                    }
                }
                FeatureEvidenceStatus::Pending => {
                    if anchor != FEATURE_EVIDENCE_DASH {
                        defect = Some(FeatureEvidenceFinding::PendingWithAnchor {
                            feature,
                            frontend,
                            line,
                        });
                    } else if note.is_empty() || note == FEATURE_EVIDENCE_DASH {
                        defect = Some(FeatureEvidenceFinding::MissingPendingNote {
                            feature,
                            frontend,
                            line,
                        });
                    }
                }
            }
            if let Some(finding) = defect {
                findings.push(finding);
                continue;
            }
            if rows
                .iter()
                .any(|row| row.feature == feature && row.frontend == frontend)
            {
                findings.push(FeatureEvidenceFinding::DuplicateCell {
                    feature,
                    frontend,
                    line,
                });
                continue;
            }
            rows.push(FeatureEvidenceRow {
                feature,
                frontend,
                status,
                anchor: if status == FeatureEvidenceStatus::Anchored {
                    anchor
                } else {
                    ""
                },
                note: if status == FeatureEvidenceStatus::Pending {
                    note
                } else {
                    ""
                },
                line,
            });
        }
        if rows.is_empty() && findings.is_empty() {
            findings.push(FeatureEvidenceFinding::EmptyTable);
        }
        Self { rows, findings }
    }

    /// Every parsed row, in committed order.
    #[must_use]
    pub fn rows(&self) -> &[FeatureEvidenceRow] {
        &self.rows
    }

    /// Every structural defect found while parsing; empty is the only passing
    /// state.
    #[must_use]
    pub fn findings(&self) -> &[FeatureEvidenceFinding] {
        &self.findings
    }

    /// The rows for one frontend, in committed order.
    pub fn rows_for(&self, frontend: FrontendShape) -> impl Iterator<Item = &FeatureEvidenceRow> {
        self.rows.iter().filter(move |row| row.frontend == frontend)
    }

    /// The committed anchor for one cell, if the row is anchored.
    #[must_use]
    pub fn anchor(&self, feature: FeatureId, frontend: FrontendShape) -> Option<&'static str> {
        self.rows
            .iter()
            .find(|row| {
                row.feature == feature
                    && row.frontend == frontend
                    && row.status == FeatureEvidenceStatus::Anchored
            })
            .map(|row| row.anchor)
    }

    /// The explicit gap reason for one cell, if the row is pending.
    #[must_use]
    pub fn pending_note(
        &self,
        feature: FeatureId,
        frontend: FrontendShape,
    ) -> Option<&'static str> {
        self.rows
            .iter()
            .find(|row| {
                row.feature == feature
                    && row.frontend == frontend
                    && row.status == FeatureEvidenceStatus::Pending
            })
            .map(|row| row.note)
    }

    /// Number of anchored rows.
    #[must_use]
    pub fn anchored_count(&self) -> usize {
        self.rows.iter().filter(|row| row.is_anchored()).count()
    }

    /// Number of pending rows.
    #[must_use]
    pub fn pending_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row.status == FeatureEvidenceStatus::Pending)
            .count()
    }
}

/// Resolve one committed `feature_id` against the [`FeatureId`] registry.
fn feature_by_id(id: &str) -> Option<FeatureId> {
    FeatureId::ALL
        .iter()
        .copied()
        .find(|feature| feature.id() == id)
}

/// Resolve one committed `frontend` against the [`FrontendShape`] registry.
fn frontend_by_name(name: &str) -> Option<FrontendShape> {
    FrontendShape::ALL
        .iter()
        .copied()
        .find(|shape| shape.name() == name)
}

/// Whether one feature's static source commitment is complete on one platform:
/// a non-empty `Requires` binding whose every capability is `Present`.
///
/// This is the predicate the fold tests before it can reach a delivery state,
/// so an anchor attached through the policy's evidence closure can only land on
/// a cell that is not refused for a delivery reason.
fn source_commitment_is_complete(
    feature: FeatureId,
    platform: PlatformAxis,
    sources: &PlatformCapabilitySurface,
) -> bool {
    match feature.platform_binding() {
        PlatformBinding::Requires(capabilities) if !capabilities.is_empty() => capabilities
            .iter()
            .all(|capability| sources.source(platform, capability) == PlatformSource::Present),
        _ => false,
    }
}

impl FeatureEvidenceTable {
    /// The anchor attached to one folded cell: the committed test id when the
    /// feature's whole `Requires` capability set is `Present` on `platform`,
    /// and [`NO_EVIDENCE`] everywhere else.
    ///
    /// Attaching only at complete source commitments keeps G4 by construction:
    /// a `Missing`, `Unsupported`, or `Gated` cell can never carry an anchor,
    /// while a `Ready` cell without a committed row stays refused by G2. A
    /// `pending` row is an explicit gap and answers [`NO_EVIDENCE`] here too.
    #[must_use]
    pub fn ledger_anchor(
        &self,
        sources: &PlatformCapabilitySurface,
        feature: FeatureId,
        frontend: FrontendShape,
        platform: PlatformAxis,
    ) -> &'static str {
        if !source_commitment_is_complete(feature, platform, sources) {
            return NO_EVIDENCE;
        }
        self.anchor(feature, frontend).unwrap_or(NO_EVIDENCE)
    }
}
