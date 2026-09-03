//! Platform-neutral result types for one-shot window capture.
//!
//! The native adapter owns the capture mechanism.  Only an owned receipt and
//! typed failure cross into application-owned request lifecycles; no Wayland,
//! D-Bus, PipeWire, or child-process type appears here.

use std::fmt;
use std::sync::Arc;

pub const MAX_WINDOW_CAPTURE_FAILURE_CHARS: usize = 512;

/// The native mechanism that produced one accepted PNG.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WindowCaptureBackend {
    /// In-process GPU framebuffer readback (zero external dependencies).
    InProcess,
    /// XDG Screenshot Portal with the active-window target.
    PortalScreenshot,
    /// KDE Spectacle's fixed-argument active-window capture path.
    SpectacleActiveWindow,
    /// Reserved for the future continuous ScreenCast/PipeWire backend.
    PipeWireScreenCast,
}

impl WindowCaptureBackend {
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::InProcess => "in-process",
            Self::PortalScreenshot => "portal-screenshot",
            Self::SpectacleActiveWindow => "spectacle-active-window",
            Self::PipeWireScreenCast => "pipewire-screencast",
        }
    }
}

/// Validated dimensions and provenance for one native PNG capture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WindowCaptureReceipt {
    width: u32,
    height: u32,
    backend: WindowCaptureBackend,
}

impl WindowCaptureReceipt {
    #[must_use]
    pub const fn new(width: u32, height: u32, backend: WindowCaptureBackend) -> Self {
        Self {
            width,
            height,
            backend,
        }
    }

    #[must_use]
    pub const fn width(self) -> u32 {
        self.width
    }

    #[must_use]
    pub const fn height(self) -> u32 {
        self.height
    }

    #[must_use]
    pub const fn backend(self) -> WindowCaptureBackend {
        self.backend
    }
}

/// Mutually distinguishable native capture failure classes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WindowCaptureFailureKind {
    Unsupported,
    ProviderUnavailable,
    PermissionDenied,
    Cancelled,
    TimedOut,
    InvalidImage,
    Io,
    ProviderFault,
}

impl WindowCaptureFailureKind {
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Unsupported => "unsupported",
            Self::ProviderUnavailable => "provider_unavailable",
            Self::PermissionDenied => "permission_denied",
            Self::Cancelled => "cancelled",
            Self::TimedOut => "timed_out",
            Self::InvalidImage => "invalid_image",
            Self::Io => "io",
            Self::ProviderFault => "provider_fault",
        }
    }
}

/// Owned, bounded detail attached to a native capture failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowCaptureFailure {
    kind: WindowCaptureFailureKind,
    detail: Arc<str>,
}

impl WindowCaptureFailure {
    #[must_use]
    pub fn new(kind: WindowCaptureFailureKind, detail: impl Into<Arc<str>>) -> Self {
        let detail = detail.into();
        let detail = if detail.chars().count() > MAX_WINDOW_CAPTURE_FAILURE_CHARS {
            Arc::from(
                detail
                    .chars()
                    .take(MAX_WINDOW_CAPTURE_FAILURE_CHARS)
                    .collect::<String>(),
            )
        } else {
            detail
        };
        Self { kind, detail }
    }

    #[must_use]
    pub const fn kind(&self) -> WindowCaptureFailureKind {
        self.kind
    }

    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for WindowCaptureFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl std::error::Error for WindowCaptureFailure {}
