//! Toolkit-neutral window presentation contracts.
//!
//! This module describes the requested surface role without owning a Wayland
//! connection, a toolkit window, an event loop, or a renderer. Each frontend
//! adapts [`WindowPresentation`] to its own standalone or layer-shell host.

use std::fmt;
use std::ops::{BitOr, BitOrAssign};

/// Requested role for one frontend-owned surface.
///
/// The role is per surface rather than process-wide. A future composition may
/// therefore keep a standalone main window and create a separate layer-shell
/// panel without changing the application projection contract.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum WindowPresentation {
    /// A normal desktop window, normally backed by `xdg_toplevel` on Wayland.
    Standalone,
    /// An optional Wayland layer-shell surface with its neutral settings.
    LayerShell(LayerShellSpec),
}

impl WindowPresentation {
    /// Return the normal desktop-window presentation.
    #[must_use]
    pub const fn standalone() -> Self {
        Self::Standalone
    }

    /// Wrap one validated layer-shell specification.
    #[must_use]
    pub const fn layer_shell(spec: LayerShellSpec) -> Self {
        Self::LayerShell(spec)
    }

    /// Borrow the layer-shell settings when this surface requests that role.
    #[must_use]
    pub const fn as_layer_shell(&self) -> Option<&LayerShellSpec> {
        match self {
            Self::Standalone => None,
            Self::LayerShell(spec) => Some(spec),
        }
    }
}

/// Neutral layer-shell settings shared by the frontend host adapters.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LayerShellSpec {
    layer: LayerShellLayer,
    anchor: LayerShellAnchor,
    size: LayerShellSize,
    margins: LayerShellMargins,
    exclusive_zone: i32,
    keyboard_interactivity: LayerShellKeyboardInteractivity,
    output: LayerShellOutput,
    namespace: String,
    fallback: LayerShellFallbackPolicy,
}

impl LayerShellSpec {
    /// Construct a top-panel profile with a compositor-selected width.
    ///
    /// The default is intentionally conservative: it does not reserve an
    /// exclusive zone and does not request keyboard focus. A frontend can opt
    /// into panel reservation or on-demand keyboard interaction explicitly.
    pub fn new(namespace: impl Into<String>) -> Result<Self, LayerShellSpecError> {
        let namespace = namespace.into();
        if namespace.trim().is_empty() {
            return Err(LayerShellSpecError::EmptyNamespace);
        }

        Ok(Self {
            layer: LayerShellLayer::Top,
            anchor: LayerShellAnchor::TOP
                .union(LayerShellAnchor::LEFT)
                .union(LayerShellAnchor::RIGHT),
            // A zero width is compositor-selected because the profile is
            // anchored to both horizontal edges. The height is explicit: a
            // zero height would require both vertical anchors by protocol.
            size: LayerShellSize::new(0, 32),
            margins: LayerShellMargins::default(),
            exclusive_zone: 0,
            keyboard_interactivity: LayerShellKeyboardInteractivity::None,
            output: LayerShellOutput::any(),
            namespace,
            fallback: LayerShellFallbackPolicy::NormalWindow,
        })
    }

    /// Construct the fixed-size desktop-widget profile used by the optional
    /// GPUI layer-shell surface.
    ///
    /// This is deliberately a separate profile from [`Self::new`]. A panel
    /// stretches across an output, while a widget owns a bounded rectangle;
    /// keeping both constructors explicit prevents a geometry smoke test from
    /// becoming the product default by accident.
    pub fn desktop_widget(namespace: impl Into<String>) -> Result<Self, LayerShellSpecError> {
        let mut spec = Self::new(namespace)?;
        spec.anchor = LayerShellAnchor::TOP.union(LayerShellAnchor::RIGHT);
        spec.size = LayerShellSize::new(520, 360);
        spec.margins = LayerShellMargins::new(16, 16, 16, 16);
        Ok(spec)
    }

    /// Validate the complete value before handing it to a native adapter.
    pub fn validate(&self) -> Result<(), LayerShellSpecError> {
        if self.namespace.trim().is_empty() {
            return Err(LayerShellSpecError::EmptyNamespace);
        }
        if self.exclusive_zone < -1 {
            return Err(LayerShellSpecError::InvalidExclusiveZone(
                self.exclusive_zone,
            ));
        }
        if self
            .output
            .name()
            .is_some_and(|name| name.trim().is_empty())
        {
            return Err(LayerShellSpecError::EmptyOutputName);
        }
        if self.size.width() == 0
            && !(self.anchor.contains(LayerShellAnchor::LEFT)
                && self.anchor.contains(LayerShellAnchor::RIGHT))
        {
            return Err(LayerShellSpecError::InvalidAnchorForZeroWidth);
        }
        if self.size.height() == 0
            && !(self.anchor.contains(LayerShellAnchor::TOP)
                && self.anchor.contains(LayerShellAnchor::BOTTOM))
        {
            return Err(LayerShellSpecError::InvalidAnchorForZeroHeight);
        }
        Ok(())
    }

    #[must_use]
    pub const fn layer(&self) -> LayerShellLayer {
        self.layer
    }

    #[must_use]
    pub const fn anchor(&self) -> LayerShellAnchor {
        self.anchor
    }

    #[must_use]
    pub const fn size(&self) -> LayerShellSize {
        self.size
    }

    #[must_use]
    pub const fn margins(&self) -> LayerShellMargins {
        self.margins
    }

    #[must_use]
    pub const fn exclusive_zone(&self) -> i32 {
        self.exclusive_zone
    }

    #[must_use]
    pub const fn keyboard_interactivity(&self) -> LayerShellKeyboardInteractivity {
        self.keyboard_interactivity
    }

    #[must_use]
    pub const fn output(&self) -> &LayerShellOutput {
        &self.output
    }

    #[must_use]
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    #[must_use]
    pub const fn fallback(&self) -> LayerShellFallbackPolicy {
        self.fallback
    }

    #[must_use]
    pub const fn with_layer(mut self, layer: LayerShellLayer) -> Self {
        self.layer = layer;
        self
    }

    #[must_use]
    pub const fn with_anchor(mut self, anchor: LayerShellAnchor) -> Self {
        self.anchor = anchor;
        self
    }

    #[must_use]
    pub const fn with_size(mut self, width: u32, height: u32) -> Self {
        self.size = LayerShellSize::new(width, height);
        self
    }

    #[must_use]
    pub const fn with_margins(mut self, margins: LayerShellMargins) -> Self {
        self.margins = margins;
        self
    }

    /// Set the protocol's exclusive-zone value.
    ///
    /// `-1` means extend to the anchored edges, `0` means do not reserve
    /// exclusive space, and non-negative values reserve compositor space.
    pub fn with_exclusive_zone(mut self, exclusive_zone: i32) -> Result<Self, LayerShellSpecError> {
        if exclusive_zone < -1 {
            return Err(LayerShellSpecError::InvalidExclusiveZone(exclusive_zone));
        }
        self.exclusive_zone = exclusive_zone;
        Ok(self)
    }

    #[must_use]
    pub const fn with_keyboard_interactivity(
        mut self,
        keyboard_interactivity: LayerShellKeyboardInteractivity,
    ) -> Self {
        self.keyboard_interactivity = keyboard_interactivity;
        self
    }

    #[must_use]
    pub fn with_output(mut self, output: LayerShellOutput) -> Self {
        self.output = output;
        self
    }

    #[must_use]
    pub const fn with_fallback(mut self, fallback: LayerShellFallbackPolicy) -> Self {
        self.fallback = fallback;
        self
    }
}

/// Invalid or incomplete layer-shell configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LayerShellSpecError {
    /// The layer-shell namespace is empty or whitespace-only.
    EmptyNamespace,
    /// An explicitly named output is empty or whitespace-only.
    EmptyOutputName,
    /// The exclusive zone is below the protocol's `-1` lower bound.
    InvalidExclusiveZone(i32),
    /// A compositor-selected width needs both horizontal anchors.
    InvalidAnchorForZeroWidth,
    /// A compositor-selected height needs both vertical anchors.
    InvalidAnchorForZeroHeight,
}

impl fmt::Display for LayerShellSpecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyNamespace => formatter.write_str("layer-shell namespace is empty"),
            Self::EmptyOutputName => formatter.write_str("layer-shell output name is empty"),
            Self::InvalidExclusiveZone(value) => {
                write!(formatter, "layer-shell exclusive zone is invalid: {value}")
            }
            Self::InvalidAnchorForZeroWidth => {
                formatter.write_str("layer-shell zero width requires left and right anchors")
            }
            Self::InvalidAnchorForZeroHeight => {
                formatter.write_str("layer-shell zero height requires top and bottom anchors")
            }
        }
    }
}

impl std::error::Error for LayerShellSpecError {}

/// Z-order requested from the compositor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LayerShellLayer {
    Background,
    Bottom,
    Top,
    Overlay,
}

/// Edge mask used by layer-shell's anchor request.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct LayerShellAnchor(u8);

impl LayerShellAnchor {
    pub const NONE: Self = Self(0);
    pub const TOP: Self = Self(1 << 0);
    pub const RIGHT: Self = Self(1 << 1);
    pub const BOTTOM: Self = Self(1 << 2);
    pub const LEFT: Self = Self(1 << 3);
    pub const ALL: Self = Self(Self::TOP.0 | Self::RIGHT.0 | Self::BOTTOM.0 | Self::LEFT.0);

    #[must_use]
    pub const fn from_bits(bits: u8) -> Option<Self> {
        if bits & !Self::ALL.0 == 0 {
            Some(Self(bits))
        } else {
            None
        }
    }

    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }

    #[must_use]
    pub const fn contains(self, edge: Self) -> bool {
        self.0 & edge.0 == edge.0
    }

    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl BitOr for LayerShellAnchor {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        self.union(rhs)
    }
}

impl BitOrAssign for LayerShellAnchor {
    fn bitor_assign(&mut self, rhs: Self) {
        *self = self.union(rhs);
    }
}

/// Initial buffer dimensions requested from the compositor.
///
/// A zero component leaves that axis under layer-shell/compositor policy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct LayerShellSize {
    width: u32,
    height: u32,
}

impl LayerShellSize {
    #[must_use]
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    #[must_use]
    pub const fn width(self) -> u32 {
        self.width
    }

    #[must_use]
    pub const fn height(self) -> u32 {
        self.height
    }
}

/// Signed layer-shell margins in top/right/bottom/left order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct LayerShellMargins {
    top: i32,
    right: i32,
    bottom: i32,
    left: i32,
}

impl LayerShellMargins {
    #[must_use]
    pub const fn new(top: i32, right: i32, bottom: i32, left: i32) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }

    #[must_use]
    pub const fn top(self) -> i32 {
        self.top
    }

    #[must_use]
    pub const fn right(self) -> i32 {
        self.right
    }

    #[must_use]
    pub const fn bottom(self) -> i32 {
        self.bottom
    }

    #[must_use]
    pub const fn left(self) -> i32 {
        self.left
    }
}

/// Keyboard focus policy requested from the compositor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LayerShellKeyboardInteractivity {
    None,
    Exclusive,
    OnDemand,
}

/// Output selection without exposing a compositor-specific `wl_output`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct LayerShellOutput {
    name: Option<String>,
}

impl LayerShellOutput {
    #[must_use]
    pub const fn any() -> Self {
        Self { name: None }
    }

    pub fn named(name: impl Into<String>) -> Result<Self, LayerShellSpecError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(LayerShellSpecError::EmptyOutputName);
        }
        Ok(Self { name: Some(name) })
    }

    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
}

/// Policy when the requested layer-shell global or capability is unavailable.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LayerShellFallbackPolicy {
    /// Re-run the frontend through its existing standalone window host.
    #[default]
    NormalWindow,
    /// Preserve the requested role and return a typed unavailable outcome.
    Unavailable,
}

#[cfg(test)]
#[path = "../tests/headless/presentation_tests.rs"]
mod tests;
