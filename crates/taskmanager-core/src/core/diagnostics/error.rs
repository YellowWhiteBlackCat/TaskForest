//! Stable failure taxonomy for diagnostic-bundle preparation and export.

use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticBundleErrorKind {
    InvalidSource,
    InvalidTarget,
    Encode,
    Io,
    Busy,
    Unavailable,
}

impl DiagnosticBundleErrorKind {
    #[must_use]
    pub const fn stable_code(self) -> &'static str {
        match self {
            Self::InvalidSource => "invalid_source",
            Self::InvalidTarget => "invalid_target",
            Self::Encode => "encode",
            Self::Io => "io",
            Self::Busy => "busy",
            Self::Unavailable => "unavailable",
        }
    }
}

/// Platform-neutral failure plus an optional provider detail.
///
/// `Display` intentionally emits only the stable kind. Callers that own a
/// trusted log or explicitly reviewed surface may opt into [`Self::detail`];
/// normal UI feedback must map [`Self::kind`] to localized copy instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticBundleError {
    kind: DiagnosticBundleErrorKind,
    detail: Option<String>,
}

impl DiagnosticBundleError {
    #[must_use]
    pub const fn new(kind: DiagnosticBundleErrorKind) -> Self {
        Self { kind, detail: None }
    }

    #[must_use]
    pub fn with_detail(kind: DiagnosticBundleErrorKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: Some(detail.into()),
        }
    }

    #[must_use]
    pub const fn kind(&self) -> DiagnosticBundleErrorKind {
        self.kind
    }

    #[must_use]
    pub fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }

    pub(super) fn encode(error: serde_json::Error) -> Self {
        Self::with_detail(DiagnosticBundleErrorKind::Encode, error.to_string())
    }
}

impl fmt::Display for DiagnosticBundleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.kind.stable_code())
    }
}

impl std::error::Error for DiagnosticBundleError {}

#[cfg(test)]
#[path = "../../../tests/headless/core_core_diagnostics_error_tests.rs"]
mod tests;
