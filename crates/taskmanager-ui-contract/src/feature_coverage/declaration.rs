//! Frontend feature-coverage declarations, reports, and drift audit.

use crate::capabilities::CapabilitySupport;
use crate::keybindings::FrontendShape;

use super::FeatureId;

/// One declared feature-to-support pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeatureCoverageEntry {
    pub feature: FeatureId,
    pub support: CapabilitySupport,
}

/// A frontend's explicit declaration of its feature coverage: one entry per
/// contract-known feature.
#[derive(Clone, Debug)]
pub struct FeatureCoverageDeclaration {
    pub frontend: FrontendShape,
    pub entries: Vec<FeatureCoverageEntry>,
}

/// The coverage outcome for one feature.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeatureCoverageStatus {
    /// An explicit support decision.
    Declared(CapabilitySupport),
    /// Contract-known but absent from the declaration - a silent omission.
    Missing,
    /// Declared more than once by the same frontend.
    Duplicated,
    /// Declared but outside the known feature set.
    Unknown,
}

impl FeatureCoverageStatus {
    /// Whether the feature carries an explicit decision - the only status a
    /// no-drift declaration may show.
    #[must_use]
    pub const fn is_explicit(self) -> bool {
        matches!(self, Self::Declared(_))
    }

    /// Whether this status is a drift finding a gate must reject.
    #[must_use]
    pub const fn is_drift(self) -> bool {
        !self.is_explicit()
    }
}

/// What a feature-coverage gate rejects, beyond the plain drift statuses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeatureCoverageFindingKind {
    /// Contract-known feature absent from the declaration.
    Missing,
    /// Feature declared more than once.
    Duplicated,
    /// Declared feature outside the known set.
    Unknown,
    /// A non-reference shape declared `Reference` (reusing the GPUI-05 rule
    /// from the capability registry: only the GPUI shape owns reference
    /// semantics).
    ReferenceOutsideReferenceShape,
    /// The reference shape declared `Ported`/`Divergent`/`Native` - the
    /// shape that owns the semantics cannot port, diverge from, or delegate
    /// itself. It MAY declare [`CapabilitySupport::Unsupported`] ("reference
    /// surface not yet delivered"): the coverage matrix keeps every gap
    /// visible, and the reference shape honestly states its own undelivered
    /// items instead of fabricating a `Reference` claim.
    ReferenceShapeCannotPort,
    /// A `Native`/`Divergent`/`Unsupported` decision without its required
    /// supplier/reason text.
    EmptyExplanation,
}

/// One gate finding: the frontend, the feature, and what is wrong.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeatureCoverageFinding {
    pub frontend: FrontendShape,
    pub feature: FeatureId,
    pub kind: FeatureCoverageFindingKind,
}

/// The coverage matrix for one declaration against the full feature set, in
/// canonical [`FeatureId::ALL`] order.
#[must_use]
pub fn feature_coverage_report(
    declaration: &FeatureCoverageDeclaration,
) -> Vec<(FeatureId, FeatureCoverageStatus)> {
    feature_coverage_report_over(declaration, FeatureId::ALL)
}

/// The drift findings alone.
#[must_use]
pub fn feature_coverage_drift(
    report: &[(FeatureId, FeatureCoverageStatus)],
) -> Vec<(FeatureId, FeatureCoverageStatus)> {
    report
        .iter()
        .copied()
        .filter(|(_, status)| status.is_drift())
        .collect()
}

/// The one-call gate payload: fold drift plus the shape-discipline rules
/// (reference role, reference-shape totality, non-empty explanations), reusing
/// the capability-registry semantics. An empty result is the only passing
/// state.
#[must_use]
pub fn feature_coverage_findings(
    declaration: &FeatureCoverageDeclaration,
) -> Vec<FeatureCoverageFinding> {
    let drift_kind = |status: FeatureCoverageStatus| match status {
        FeatureCoverageStatus::Missing => Some(FeatureCoverageFindingKind::Missing),
        FeatureCoverageStatus::Duplicated => Some(FeatureCoverageFindingKind::Duplicated),
        FeatureCoverageStatus::Unknown => Some(FeatureCoverageFindingKind::Unknown),
        FeatureCoverageStatus::Declared(_) => None,
    };
    let mut findings: Vec<FeatureCoverageFinding> = feature_coverage_report(declaration)
        .into_iter()
        .filter_map(|(feature, status)| {
            drift_kind(status).map(|kind| FeatureCoverageFinding {
                frontend: declaration.frontend,
                feature,
                kind,
            })
        })
        .collect();
    for (feature, status) in feature_coverage_report(declaration) {
        let FeatureCoverageStatus::Declared(support) = status else {
            continue;
        };
        let kind = if support == CapabilitySupport::Reference
            && !declaration.frontend.is_capability_reference_shape()
        {
            Some(FeatureCoverageFindingKind::ReferenceOutsideReferenceShape)
        } else if declaration.frontend.is_capability_reference_shape()
            && matches!(
                support,
                CapabilitySupport::Ported
                    | CapabilitySupport::Divergent { .. }
                    | CapabilitySupport::Native { .. }
            )
        {
            Some(FeatureCoverageFindingKind::ReferenceShapeCannotPort)
        } else if support.explanation().is_some_and(str::is_empty) {
            Some(FeatureCoverageFindingKind::EmptyExplanation)
        } else {
            None
        };
        if let Some(kind) = kind {
            findings.push(FeatureCoverageFinding {
                frontend: declaration.frontend,
                feature,
                kind,
            });
        }
    }
    findings.sort_by_key(|finding| finding.feature);
    findings.dedup();
    findings
}

/// The fold against a restricted known set - the seam that keeps the `Unknown`
/// path real (a declared feature outside `known` is reported, never silently
/// dropped).
#[must_use]
pub(crate) fn feature_coverage_report_over(
    declaration: &FeatureCoverageDeclaration,
    known: &[FeatureId],
) -> Vec<(FeatureId, FeatureCoverageStatus)> {
    let mut report: Vec<(FeatureId, FeatureCoverageStatus)> = known
        .iter()
        .map(|feature| (*feature, FeatureCoverageStatus::Missing))
        .collect();
    for entry in &declaration.entries {
        match report
            .iter_mut()
            .find(|(feature, _)| *feature == entry.feature)
        {
            Some((_, status)) => {
                if status.is_explicit() || matches!(status, FeatureCoverageStatus::Duplicated) {
                    *status = FeatureCoverageStatus::Duplicated;
                } else {
                    *status = FeatureCoverageStatus::Declared(entry.support);
                }
            }
            None => report.push((entry.feature, FeatureCoverageStatus::Unknown)),
        }
    }
    report
}
