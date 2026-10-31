//! Platform-neutral identity facts for a process that belongs to a desktop app.

use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize};

use super::ProcessItem;
use super::metadata::{ProcessMetadataAvailability, ProcessMetadataFailure};

/// Maximum bytes retained for one resolved desktop icon.
///
/// The Linux provider applies the same bound before this value crosses the
/// provider boundary. Keeping the constructor bounded as well protects
/// deserialized snapshots and fixture callers from turning a process row into
/// an unbounded image transport.
pub const MAX_APPLICATION_ICON_BYTES: usize = 512 * 1024;

/// Image formats that a platform provider may resolve for a desktop icon.
///
/// This is deliberately toolkit-neutral. The GPUI adapter maps it to its
/// native image format only at the composition/render boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplicationIconFormat {
    Svg,
    Png,
    Jpeg,
    Webp,
    Bmp,
}

/// A bounded, provider-resolved desktop icon payload.
///
/// Icon bytes are immutable and reference-counted so an Apps group can share
/// one asset across its process rows. No Linux path, theme directory, or
/// filesystem handle crosses this boundary.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct ApplicationIconAsset {
    pub format: ApplicationIconFormat,
    #[serde(default)]
    pub bytes: Arc<[u8]>,
    /// Stable content key used by frontends to cache decoded image objects.
    pub content_hash: u64,
}

#[derive(Debug, Deserialize)]
struct ApplicationIconAssetWire {
    format: ApplicationIconFormat,
    bytes: Arc<[u8]>,
    content_hash: u64,
}

impl<'de> Deserialize<'de> for ApplicationIconAsset {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ApplicationIconAssetWire::deserialize(deserializer)?;
        if wire.bytes.is_empty() || wire.bytes.len() > MAX_APPLICATION_ICON_BYTES {
            return Err(serde::de::Error::custom(
                "application icon exceeds byte bound",
            ));
        }
        let expected_hash = stable_icon_hash(&wire.bytes);
        if wire.content_hash != expected_hash {
            return Err(serde::de::Error::custom(
                "application icon content hash does not match bytes",
            ));
        }
        if !format_matches_bytes(wire.format, &wire.bytes) {
            return Err(serde::de::Error::custom(
                "application icon bytes do not match declared format",
            ));
        }
        Ok(Self {
            format: wire.format,
            bytes: wire.bytes,
            content_hash: wire.content_hash,
        })
    }
}

impl ApplicationIconAsset {
    /// Build a bounded asset and derive its stable content key.
    #[must_use]
    pub fn from_bytes(format: ApplicationIconFormat, bytes: Vec<u8>) -> Option<Self> {
        if bytes.is_empty()
            || bytes.len() > MAX_APPLICATION_ICON_BYTES
            || !format_matches_bytes(format, &bytes)
        {
            return None;
        }
        let content_hash = stable_icon_hash(&bytes);
        Some(Self {
            format,
            bytes: Arc::from(bytes),
            content_hash,
        })
    }
}

fn format_matches_bytes(format: ApplicationIconFormat, bytes: &[u8]) -> bool {
    match format {
        ApplicationIconFormat::Svg => String::from_utf8_lossy(&bytes[..bytes.len().min(4096)])
            .to_ascii_lowercase()
            .contains("<svg"),
        ApplicationIconFormat::Png => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        ApplicationIconFormat::Jpeg => bytes.starts_with(&[0xff, 0xd8, 0xff]),
        ApplicationIconFormat::Webp => {
            bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP")
        }
        ApplicationIconFormat::Bmp => bytes.starts_with(b"BM"),
    }
}

fn stable_icon_hash(bytes: &[u8]) -> u64 {
    // FNV-1a is sufficient here: this is a bounded cache key, not a security
    // primitive or an identity proof.
    bytes.iter().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}

/// Identity resolved from a verified application-launcher association.
///
/// `icon_token` is a provider-selected icon token, not a filesystem path. The
/// resolved [`ApplicationIconAsset`] is the only data a frontend may render;
/// a token without an asset remains an honest generic-glyph fallback.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProcessApplicationIdentity {
    /// Stable launcher identity, for example a desktop-file id or bundle id.
    pub launcher_id: String,
    /// Display name selected by the provider's locale policy.
    pub display_name: String,
    /// Optional provider-neutral icon token associated with the launcher.
    #[serde(default)]
    pub icon_token: Option<String>,
    /// Resolved icon bytes, when the provider found and validated the token.
    #[serde(default)]
    pub icon_asset: Option<ApplicationIconAsset>,
    /// Explicit icon resolution failure. This is separate from application
    /// identity: grouping remains valid when the icon theme is unavailable.
    #[serde(default)]
    pub icon_failure: Option<ProcessMetadataFailure>,
}

impl ProcessApplicationIdentity {
    /// Build an identity only when the desktop id and display name are real.
    ///
    /// An absent icon is intentional: desktop entries are allowed to omit
    /// `Icon=`, and the Linux provider reports that case as a partial
    /// observation. Keeping the identity itself valid lets callers group the
    /// process without pretending that a system icon was resolved.
    #[must_use]
    pub fn new(
        launcher_id: impl Into<String>,
        display_name: impl Into<String>,
        icon_token: Option<String>,
    ) -> Option<Self> {
        let launcher_id = launcher_id.into().trim().to_owned();
        let display_name = display_name.into().trim().to_owned();
        (!launcher_id.trim().is_empty() && !display_name.trim().is_empty()).then_some(Self {
            launcher_id,
            display_name,
            icon_token: icon_token.filter(|icon| !icon.trim().is_empty()),
            icon_asset: None,
            icon_failure: None,
        })
    }

    /// Attach a provider-resolved asset or its typed failure without changing
    /// the launcher identity used for grouping and matching.
    pub fn with_icon_resolution(
        mut self,
        asset: Option<ApplicationIconAsset>,
        failure: Option<ProcessMetadataFailure>,
    ) -> Self {
        self.icon_asset = asset;
        self.icon_failure = failure;
        self
    }

    /// Whether the provider supplied a real icon token for this identity.
    ///
    /// The token is deliberately opaque. Resolving it to pixels belongs to a
    /// later asset boundary and must not be inferred from this boolean.
    #[must_use]
    pub const fn has_icon_token(&self) -> bool {
        self.icon_token.is_some()
    }

    /// Whether the identity carries validated bytes that can be rendered.
    #[must_use]
    pub const fn has_icon_asset(&self) -> bool {
        self.icon_asset.is_some()
    }
}

/// Platform-neutral first-level process grouping for the process table.
///
/// The category is a *confirmed* fact derived from the typed application
/// identity observation: only a current identity proves
/// [`ProcessCategory::Application`], only a current confirmed absence proves
/// [`ProcessCategory::Background`], and every not-current state (unknown,
/// stale, unavailable) stays in the honest [`ProcessCategory::Uncategorized`]
/// bucket instead of being fabricated into either side.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum ProcessCategory {
    Application,
    Background,
    Uncategorized,
}

impl ProcessCategory {
    /// The complete variant list in canonical evaluation order
    /// (Application → Background → Uncategorized). Tests and frontends
    /// iterate this constant instead of re-enumerating the variants, so the
    /// list exists exactly once.
    pub const ALL: [Self; 3] = [Self::Application, Self::Background, Self::Uncategorized];

    /// Stable locale-neutral key identifying this bucket in frontend
    /// expansion sets. Distinct across [`ProcessCategory::ALL`].
    #[must_use]
    pub const fn stable_key(self) -> &'static str {
        match self {
            Self::Application => "application",
            Self::Background => "background",
            Self::Uncategorized => "uncategorized",
        }
    }
}

/// Classify one process into its first-level table grouping.
///
/// Mapping, by the typed semantics of `ProcessMetadataObservation` on
/// [`ProcessItem::application_identity_observation`]:
/// - `Available` / `Partial(_)` — a current verified identity (a partial
///   observation still carries the identity; only its icon resolution
///   failed) → [`ProcessCategory::Application`].
/// - `Absent` — the provider currently confirmed the process belongs to no
///   desktop application → [`ProcessCategory::Background`].
/// - `Unknown` / `Stale(_)` / `Unavailable(_)` — the truth is not current,
///   so neither bucket is provable → [`ProcessCategory::Uncategorized`]. An
///   unknown identity is never fabricated into Background.
#[must_use]
pub fn process_category(item: &ProcessItem) -> ProcessCategory {
    match item.application_identity_observation().availability() {
        ProcessMetadataAvailability::Available | ProcessMetadataAvailability::Partial(_) => {
            ProcessCategory::Application
        }
        ProcessMetadataAvailability::Absent => ProcessCategory::Background,
        ProcessMetadataAvailability::Unknown
        | ProcessMetadataAvailability::Stale(_)
        | ProcessMetadataAvailability::Unavailable(_) => ProcessCategory::Uncategorized,
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/core_core_process_application_tests.rs"]
mod tests;
