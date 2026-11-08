//! The drift census over the declaration tables: completeness, uniqueness and
//! reason discipline, plus the unexplained-debt set and the axes that may
//! lawfully explain a differing verdict.
//!
//! These are pure folds over the declared data — never runtime availability.

use crate::keybindings::FrontendShape;
use crate::presentation::thresholds::{
    AxisDecision, DECLARATIONS, LayoutAxis, LayoutThresholdDeclaration, decision, number, optional,
};

/// One shape's drift on the threshold declaration axis: it reports whether the
/// registry is complete and self-consistent, never runtime availability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutThresholdFindingKind {
    /// The shape declares nothing for this axis: a silent omission.
    MissingAxis,
    /// The shape declares this axis twice.
    DuplicateAxis,
    /// The shape claims the shared baseline for an axis that has none.
    SharedWithoutBaseline,
    /// The shape declares its own value equal to the baseline: drift bait that
    /// must be written as `Shared`.
    RedundantOwn,
    /// `Own`/`Absent` without a driver: the reason text is the register.
    EmptyReason,
}

/// One drift finding against the layout threshold registry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayoutThresholdFinding {
    pub frontend: FrontendShape,
    pub axis: LayoutAxis,
    pub kind: LayoutThresholdFindingKind,
}

/// Validate every shape's declaration: exhaustive, unique, reason-bearing.
#[must_use]
pub fn layout_threshold_findings() -> Vec<LayoutThresholdFinding> {
    declaration_findings(DECLARATIONS)
}

/// Validate one supplied declaration table. The public gate calls this with
/// [`DECLARATIONS`]; its tests call it with mutated tables so every kind of
/// drift is detected by real behavior rather than enum equality.
#[must_use]
pub fn declaration_findings(
    declarations: &[LayoutThresholdDeclaration<'_>],
) -> Vec<LayoutThresholdFinding> {
    let mut findings = Vec::new();
    for frontend in FrontendShape::ALL {
        for axis in LayoutAxis::ALL {
            let declared: Vec<AxisDecision> = declarations
                .iter()
                .find(|candidate| candidate.frontend == frontend)
                .map(|existing| {
                    existing
                        .entries
                        .iter()
                        .filter(|candidate| candidate.axis == axis)
                        .map(|candidate| candidate.decision)
                        .collect()
                })
                .unwrap_or_default();
            match declared.len() {
                0 => findings.push(LayoutThresholdFinding {
                    frontend,
                    axis,
                    kind: LayoutThresholdFindingKind::MissingAxis,
                }),
                1 => {
                    if let Some(kind) = decision_drift(axis, declared[0]) {
                        findings.push(LayoutThresholdFinding {
                            frontend,
                            axis,
                            kind,
                        });
                    }
                }
                _ => findings.push(LayoutThresholdFinding {
                    frontend,
                    axis,
                    kind: LayoutThresholdFindingKind::DuplicateAxis,
                }),
            }
        }
    }
    findings
}

fn decision_drift(axis: LayoutAxis, decision: AxisDecision) -> Option<LayoutThresholdFindingKind> {
    match decision {
        AxisDecision::Shared if axis.baseline().is_none() => {
            Some(LayoutThresholdFindingKind::SharedWithoutBaseline)
        }
        AxisDecision::Own { reason, .. } if reason.trim().is_empty() => {
            Some(LayoutThresholdFindingKind::EmptyReason)
        }
        AxisDecision::Own { value, .. } if axis.baseline() == Some(value) => {
            Some(LayoutThresholdFindingKind::RedundantOwn)
        }
        AxisDecision::Absent { reason } if reason.trim().is_empty() => {
            Some(LayoutThresholdFindingKind::EmptyReason)
        }
        _ => None,
    }
}

/// One unexplained difference: a shape that spends a band the reference shape
/// does not, with no recorded driver. Set equality against this census is the
/// ratchet — new debt fails, retired debt must shrink the baseline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayoutDebtCell {
    pub frontend: FrontendShape,
    pub axis: LayoutAxis,
}

/// The live debt census over every declared shape and axis.
#[must_use]
pub fn layout_debt_census() -> Vec<LayoutDebtCell> {
    let mut cells = Vec::new();
    for frontend in FrontendShape::ALL {
        for axis in LayoutAxis::ALL {
            if decision(frontend, axis).is_some_and(AxisDecision::is_debt) {
                cells.push(LayoutDebtCell { frontend, axis });
            }
        }
    }
    cells
}

/// The axes on which two shapes spend a different effective band. This is the
/// only lawful explanation for a differing layout verdict: a field whose
/// governing axes are NOT in this set must classify identically.
#[must_use]
pub fn divergent_axes(left: FrontendShape, right: FrontendShape) -> Vec<LayoutAxis> {
    LayoutAxis::ALL
        .iter()
        .copied()
        .filter(|axis| effective_band(left, *axis) != effective_band(right, *axis))
        .collect()
}

/// One shape's resolved band for an axis, as the classifier actually sees it.
#[must_use]
pub fn effective_band(frontend: FrontendShape, axis: LayoutAxis) -> Option<f32> {
    if axis.is_optional() {
        optional(frontend, axis)
    } else {
        Some(number(frontend, axis))
    }
}
