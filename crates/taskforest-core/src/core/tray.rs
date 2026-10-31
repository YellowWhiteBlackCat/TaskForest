//! Neutral system-tray contract shared by every frontend and OS adapter.
//!
//! This module is the single source for tray icon, menu, and event vocabulary:
//! frontends build a [`TraySpec`], native adapters render it, and all three
//! platform adapters (Linux StatusNotifierItem, Windows tray icon, macOS
//! `NSStatusItem`) consume the same types. No OS, toolkit, or thread types
//! appear here; the spec is validated at construction so adapters can trust it.

/// Maximum edge length (pixels) of a tray icon buffer. Real tray icons are
/// 16-22 px; the cap bounds adapter work without accepting absurd inputs.
pub const MAX_TRAY_ICON_DIMENSION: u32 = 256;

/// Maximum tooltip length. Windows reserves a 128-wide `WCHAR` buffer for the
/// notification-area tooltip; this cap leaves room for the terminator.
pub const MAX_TRAY_TOOLTIP_CHARS: usize = 127;

/// Maximum StatusNotifierItem title length. Purely a bounded-work cap.
pub const MAX_TRAY_TITLE_CHARS: usize = 512;

/// Maximum label length for one menu item. Purely a bounded-work cap.
pub const MAX_TRAY_LABEL_CHARS: usize = 256;

/// Maximum total nodes (items + separators, counting every nesting level) in
/// one tray menu. Purely a bounded-work cap.
pub const MAX_TRAY_MENU_NODES: usize = 64;

/// Maximum submenu nesting depth. Both native menu systems support deeper
/// trees, but the product never needs them; the cap keeps traversal simple.
pub const MAX_TRAY_MENU_DEPTH: u8 = 3;

/// Stable id carried by an activating tray menu item. Zero is a legal id.
pub type TrayActionId = u32;

/// RGBA (non-premultiplied, 8 bits per channel) pixel buffer for the tray
/// icon. The single icon source consumed by every OS adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayIconData {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
}

/// Why a [`TrayIconData`] is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayIconError {
    /// One dimension is zero.
    EmptyDimension,
    /// One dimension exceeds [`MAX_TRAY_ICON_DIMENSION`].
    DimensionTooLarge { dimension: u32 },
    /// The pixel buffer length does not equal `width * height * 4`.
    PixelBufferLengthMismatch { expected: usize, actual: usize },
}

impl TrayIconData {
    /// Validate and construct a tray icon from raw RGBA pixels.
    pub fn from_rgba(rgba: Vec<u8>, width: u32, height: u32) -> Result<Self, TrayIconError> {
        if width == 0 || height == 0 {
            return Err(TrayIconError::EmptyDimension);
        }
        for dimension in [width, height] {
            if dimension > MAX_TRAY_ICON_DIMENSION {
                return Err(TrayIconError::DimensionTooLarge { dimension });
            }
        }
        let Some(expected) = width
            .checked_mul(height)
            .and_then(|pixels| pixels.checked_mul(4))
            .and_then(|bytes| usize::try_from(bytes).ok())
        else {
            return Err(TrayIconError::PixelBufferLengthMismatch {
                expected: usize::MAX,
                actual: rgba.len(),
            });
        };
        if rgba.len() != expected {
            return Err(TrayIconError::PixelBufferLengthMismatch {
                expected,
                actual: rgba.len(),
            });
        }
        Ok(Self {
            rgba,
            width,
            height,
        })
    }

    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        &self.rgba
    }
}

/// Discriminator of one [`TrayMenuItem`] variant, for exhaustive enumeration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayMenuItemKind {
    Action,
    Checkmark,
    Radio,
    Submenu,
    Separator,
}

impl TrayMenuItemKind {
    pub const ALL: [Self; 5] = [
        Self::Action,
        Self::Checkmark,
        Self::Radio,
        Self::Submenu,
        Self::Separator,
    ];
}

/// One neutral tray menu entry. Adapters map each variant to their native
/// menu item; unknown variants are impossible by construction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrayMenuItem {
    /// Plain action item; activating it emits [`TrayEvent::MenuActivated`].
    Action {
        id: TrayActionId,
        label: String,
        enabled: bool,
    },
    /// Toggle item with a visible check state.
    Checkmark {
        id: TrayActionId,
        label: String,
        checked: bool,
        enabled: bool,
    },
    /// Exclusive-choice item. `radio_group` groups mutually exclusive items;
    /// items with the same group id form one radio set.
    Radio {
        id: TrayActionId,
        label: String,
        checked: bool,
        enabled: bool,
        radio_group: Option<u32>,
    },
    /// Nested submenu. Depth is validated against [`MAX_TRAY_MENU_DEPTH`].
    Submenu {
        label: String,
        items: Vec<TrayMenuItem>,
        enabled: bool,
    },
    Separator,
}

impl TrayMenuItem {
    #[must_use]
    pub fn kind(&self) -> TrayMenuItemKind {
        match self {
            Self::Action { .. } => TrayMenuItemKind::Action,
            Self::Checkmark { .. } => TrayMenuItemKind::Checkmark,
            Self::Radio { .. } => TrayMenuItemKind::Radio,
            Self::Submenu { .. } => TrayMenuItemKind::Submenu,
            Self::Separator => TrayMenuItemKind::Separator,
        }
    }
}

/// Why a [`TrayMenuSpec`] is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayMenuSpecError {
    /// The menu contains more than [`MAX_TRAY_MENU_NODES`] items in total.
    TooManyNodes { nodes: usize },
    /// Nesting exceeds [`MAX_TRAY_MENU_DEPTH`].
    NestingTooDeep { depth: u8 },
    /// One label exceeds [`MAX_TRAY_LABEL_CHARS`].
    LabelTooLong { label_chars: usize },
    /// Two radio items of the same group are both checked; a group must have
    /// at most one selected member.
    RadioGroupConflict { group: u32 },
}

/// A validated tray menu tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayMenuSpec {
    items: Vec<TrayMenuItem>,
}

impl TrayMenuSpec {
    /// Validate and construct a menu tree.
    pub fn from_items(items: Vec<TrayMenuItem>) -> Result<Self, TrayMenuSpecError> {
        if items.len() > MAX_TRAY_MENU_NODES {
            return Err(TrayMenuSpecError::TooManyNodes { nodes: items.len() });
        }
        Self::validate(&items, 0)?;
        Ok(Self { items })
    }

    fn validate(items: &[TrayMenuItem], depth: u8) -> Result<(), TrayMenuSpecError> {
        Self::validate_with_state(items, depth, &mut std::collections::BTreeMap::new())
    }

    fn validate_with_state(
        items: &[TrayMenuItem],
        depth: u8,
        checked_radios: &mut std::collections::BTreeMap<u32, u32>,
    ) -> Result<(), TrayMenuSpecError> {
        if depth > MAX_TRAY_MENU_DEPTH {
            return Err(TrayMenuSpecError::NestingTooDeep { depth });
        }
        let mut nodes = items.len();
        for item in items {
            if let TrayMenuItem::Submenu { label, items, .. } = item {
                if label.chars().count() > MAX_TRAY_LABEL_CHARS {
                    return Err(TrayMenuSpecError::LabelTooLong {
                        label_chars: label.chars().count(),
                    });
                }
                nodes += items.len();
                if nodes > MAX_TRAY_MENU_NODES {
                    return Err(TrayMenuSpecError::TooManyNodes { nodes });
                }
                Self::validate_with_state(items, depth + 1, checked_radios)?;
            } else if let Some(label) = Self::label_of(item)
                && label.chars().count() > MAX_TRAY_LABEL_CHARS
            {
                return Err(TrayMenuSpecError::LabelTooLong {
                    label_chars: label.chars().count(),
                });
            }
            if let TrayMenuItem::Radio {
                checked: true,
                radio_group: Some(group),
                ..
            } = item
                && checked_radios.insert(*group, *group).is_some()
            {
                return Err(TrayMenuSpecError::RadioGroupConflict { group: *group });
            }
        }
        Ok(())
    }

    fn label_of(item: &TrayMenuItem) -> Option<&str> {
        match item {
            TrayMenuItem::Action { label, .. }
            | TrayMenuItem::Checkmark { label, .. }
            | TrayMenuItem::Radio { label, .. } => Some(label),
            TrayMenuItem::Submenu { .. } | TrayMenuItem::Separator => None,
        }
    }

    #[must_use]
    pub fn items(&self) -> &[TrayMenuItem] {
        &self.items
    }

    /// Total node count including every nesting level.
    #[must_use]
    pub fn node_count(&self) -> usize {
        let mut count = 0;
        let mut pending = self.items.iter().collect::<Vec<_>>();
        while let Some(item) = pending.pop() {
            count += 1;
            if let TrayMenuItem::Submenu { items, .. } = item {
                pending.extend(items.iter());
            }
        }
        count
    }

    /// Whether any item can emit an activation event.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// Why a [`TraySpec`] is invalid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TraySpecError {
    Icon(TrayIconError),
    Menu(TrayMenuSpecError),
    /// The tooltip exceeds [`MAX_TRAY_TOOLTIP_CHARS`].
    TooltipTooLong {
        tooltip_chars: usize,
    },
    /// The title exceeds [`MAX_TRAY_TITLE_CHARS`].
    TitleTooLong {
        title_chars: usize,
    },
}

/// Complete validated definition of one system tray icon.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraySpec {
    icon: TrayIconData,
    tooltip: Option<String>,
    title: Option<String>,
    menu: TrayMenuSpec,
    show_menu_on_left_click: bool,
}

impl TraySpec {
    /// Validate and construct the full tray definition.
    pub fn new(
        icon: TrayIconData,
        tooltip: Option<String>,
        title: Option<String>,
        menu: TrayMenuSpec,
        show_menu_on_left_click: bool,
    ) -> Result<Self, TraySpecError> {
        if let Some(tooltip) = &tooltip
            && tooltip.chars().count() > MAX_TRAY_TOOLTIP_CHARS
        {
            return Err(TraySpecError::TooltipTooLong {
                tooltip_chars: tooltip.chars().count(),
            });
        }
        if let Some(title) = &title
            && title.chars().count() > MAX_TRAY_TITLE_CHARS
        {
            return Err(TraySpecError::TitleTooLong {
                title_chars: title.chars().count(),
            });
        }
        Ok(Self {
            icon,
            tooltip,
            title,
            menu,
            show_menu_on_left_click,
        })
    }

    #[must_use]
    pub fn icon(&self) -> &TrayIconData {
        &self.icon
    }

    #[must_use]
    pub fn tooltip(&self) -> Option<&str> {
        self.tooltip.as_deref()
    }

    /// StatusNotifierItem title (Linux only; ignored on Windows and macOS).
    #[must_use]
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    #[must_use]
    pub fn menu(&self) -> &TrayMenuSpec {
        &self.menu
    }

    /// Whether a left click shows the menu. Unsupported on Linux; adapters
    /// ignore it there.
    #[must_use]
    pub fn show_menu_on_left_click(&self) -> bool {
        self.show_menu_on_left_click
    }
}

/// A user interaction with the tray, delivered to the frontend host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrayEvent {
    /// An action, checkmark, or radio item was activated.
    MenuActivated { id: TrayActionId },
    /// The icon itself was activated (left click on Windows, activate on
    /// StatusNotifierItem, single click on macOS).
    IconActivated,
    /// The icon was double-clicked (Windows and macOS only).
    IconDoubleClicked,
}

#[cfg(test)]
#[path = "../../tests/headless/core_core_tray_tests.rs"]
mod tests;
