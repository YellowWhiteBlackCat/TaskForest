//! Per-window Performance unit preference setters.

use gpui::Context;

use super::{MagnitudeBase, QuantityNotation, RootView, UnitFamily};

impl RootView {
    /// Snapshot the six current unit choices at render entry. Providers only
    /// publish source values; this projection remains presentation-owned.
    pub(crate) const fn display_units(&self) -> crate::gpui_app::formatting::DisplayUnits {
        self.presentation.units()
    }

    pub(crate) fn set_memory_use_bytes(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.presentation.set_quantity_notation(
            UnitFamily::Memory,
            if enabled {
                QuantityNotation::Bytes
            } else {
                QuantityNotation::Bits
            },
        );
        cx.notify();
    }

    pub(crate) fn set_memory_use_base2(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.presentation.set_magnitude_base(
            UnitFamily::Memory,
            if enabled {
                MagnitudeBase::Binary
            } else {
                MagnitudeBase::Decimal
            },
        );
        cx.notify();
    }

    pub(crate) fn set_drive_use_bytes(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.presentation.set_quantity_notation(
            UnitFamily::Drive,
            if enabled {
                QuantityNotation::Bytes
            } else {
                QuantityNotation::Bits
            },
        );
        cx.notify();
    }

    pub(crate) fn set_drive_use_base2(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.presentation.set_magnitude_base(
            UnitFamily::Drive,
            if enabled {
                MagnitudeBase::Binary
            } else {
                MagnitudeBase::Decimal
            },
        );
        cx.notify();
    }

    pub(crate) fn set_network_use_bytes(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.presentation.set_quantity_notation(
            UnitFamily::Network,
            if enabled {
                QuantityNotation::Bytes
            } else {
                QuantityNotation::Bits
            },
        );
        cx.notify();
    }

    pub(crate) fn set_network_use_base2(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.presentation.set_magnitude_base(
            UnitFamily::Network,
            if enabled {
                MagnitudeBase::Binary
            } else {
                MagnitudeBase::Decimal
            },
        );
        cx.notify();
    }
}
