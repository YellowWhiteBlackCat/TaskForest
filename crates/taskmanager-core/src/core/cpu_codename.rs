//! CPU codename and process-node lookup for probed CPUID identities.
//!
//! Pure table: `(vendor, display family, display model) -> (codename,
//! process node)`. The display pair is exactly the SDM combination exposed by
//! [`crate::core::hardware::CpuIdentity::display_family`] /
//! [`display_model`](crate::core::hardware::CpuIdentity::display_model).
//!
//! # Provenance
//!
//! Rows are transcribed from libcpuid's recognition tables
//! (`recog_intel.c` / `recog_amd.c`, upstream master), whose matcher keys on
//! the same display pair; family groupings were cross-checked against public
//! CPUID references. Rows whose pair is shared across sub-generations (e.g.
//! Intel model `0x55` covering Skylake-SP through Cooper Lake, `0x9E`
//! covering Kaby through Coffee Lake Refresh) carry combined names — a pure
//! identity lookup cannot split them without brand-string heuristics, and an
//! approximate honest label beats a fabricated precise one. Unknown pairs
//! return `None`; the caller renders an absent row.
//!
//! Process-node strings follow libcpuid's technology vocabulary verbatim
//! (e.g. `"Intel 7"`, `"TSMC N4"`, `"14++ nm"`).

/// The two vendors the codename table covers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CpuVendor {
    Intel,
    Amd,
}

impl CpuVendor {
    /// Map a native CPUID vendor string (leaf 0) onto the table's vendors.
    /// Other vendors (virtualized, Hygon, …) are not covered — `None`.
    #[must_use]
    pub fn from_vendor_id(vendor_id: &str) -> Option<Self> {
        match vendor_id {
            "GenuineIntel" => Some(Self::Intel),
            "AuthenticAMD" => Some(Self::Amd),
            _ => None,
        }
    }
}

/// Returns `(codename, process_node)` for a CPUID display family/model pair.
/// Uncovered IDs return `None` — an honest absence, never a guessed label.
#[must_use]
pub fn classify_cpu_codename(
    vendor: CpuVendor,
    display_family: u32,
    display_model: u32,
) -> Option<(&'static str, &'static str)> {
    match (vendor, display_family, display_model) {
        // =====================================================================
        // Intel — family 6, modern mainstream + Xeon Scalable (Nehalem .. Arrow/Lunar)
        // =====================================================================
        (CpuVendor::Intel, 6, m) => match m {
            // --- Nehalem (2008, 45 nm) ---
            0x1A => Some(("Nehalem (Bloomfield/Gainestown)", "45 nm")),
            0x1E => Some(("Nehalem (Lynnfield/Clarksfield)", "45 nm")),

            // --- Westmere (2010, 32 nm) ---
            0x2C => Some(("Westmere (Gulftown/Westmere-EP)", "32 nm")),
            0x25 => Some(("Westmere (Clarkdale/Arrandale)", "32 nm")),
            0x2E => Some(("Nehalem-EX (Beckton)", "32 nm")),
            0x2F => Some(("Westmere-EX", "32 nm")),

            // --- Sandy Bridge (2011, 32 nm) ---
            0x2A => Some(("Sandy Bridge", "32 nm")),
            0x2D => Some(("Sandy Bridge-E", "32 nm")),

            // --- Ivy Bridge (2012, 22 nm) ---
            0x3A => Some(("Ivy Bridge", "22 nm")),
            0x3E => Some(("Ivy Bridge-E", "22 nm")),

            // --- Haswell (2013, 22 nm) ---
            0x3C => Some(("Haswell", "22 nm")),
            0x3F => Some(("Haswell-E (Haswell-EP)", "22 nm")),
            0x45 => Some(("Haswell-ULT", "22 nm")),
            0x46 => Some(("Haswell-H", "22 nm")),

            // --- Broadwell (2014, 14 nm) ---
            0x3D => Some(("Broadwell-U", "14 nm")),
            0x47 => Some(("Broadwell-H", "14 nm")),
            0x4F => Some(("Broadwell-E (Broadwell-EP)", "14 nm")),
            0x56 => Some(("Broadwell-D (Xeon D)", "14 nm")),

            // --- Skylake (2015, 14 nm) ---
            0x4E => Some(("Skylake-U", "14 nm")),
            0x5E => Some(("Skylake-S", "14 nm")),
            // SHARED MODEL 0x55: Skylake-SP/X, Cascade Lake, and Cooper Lake
            // (stepping-split upstream) — needs brand/stepping to separate.
            0x55 => Some(("Skylake-SP/X + Cascade Lake (+ Cooper Lake)", "14 nm")),

            // --- Kaby / Coffee / Comet / Whiskey / Amber (14+/14++ nm) ---
            // SHARED MODEL 0x8E: the mobile Lake refresh chain.
            0x8E => Some(("Kaby/Amber/Whiskey/Comet Lake-U (mobile)", "14+ nm")),
            // SHARED MODEL 0x9E: the desktop Lake chain.
            0x9E => Some(("Kaby Lake-S / Coffee Lake(-R)", "14++ nm")),
            0xA5 => Some(("Comet Lake-S", "14++ nm")),
            0xA6 => Some(("Comet Lake-U", "14++ nm")),

            // --- Ice Lake (10 nm) ---
            0x7E => Some(("Ice Lake-U", "10 nm")),
            0x6A => Some(("Ice Lake-SP (Xeon Scalable 3rd)", "10 nm")),
            0x6C => Some(("Ice Lake-D (Xeon D)", "10 nm")),

            // --- Tiger Lake (10 nm SuperFin) ---
            0x8C => Some(("Tiger Lake (UP3/UP4/H35)", "10 nm SuperFin")),
            0x8D => Some(("Tiger Lake-H", "10 nm SuperFin")),

            // --- Rocket Lake (14++ nm) ---
            0xA7 => Some(("Rocket Lake-S", "14++ nm")),

            // --- Alder Lake (Intel 7) ---
            0x97 => Some(("Alder Lake-S/HX", "Intel 7")),
            0x9A => Some(("Alder Lake-P/U/H", "Intel 7")),
            // SHARED MODEL 0xBE: Alder Lake-N and its Twin Lake refresh.
            0xBE => Some(("Alder Lake-N / Twin Lake-N", "Intel 7")),

            // --- Raptor Lake (Intel 7) ---
            // 0xB7 covers 13th AND 14th gen refresh AND Xeon E-24xx.
            0xB7 => Some(("Raptor Lake-S/HX (13th/14th gen)", "Intel 7")),
            0xBA => Some(("Raptor Lake-P/U/H", "Intel 7")),
            0xBF => Some(("Raptor Lake-S (i5/i3)", "Intel 7")),

            // --- Sapphire / Emerald / Granite Rapids Xeon generations ---
            0x8F => Some(("Sapphire Rapids (Xeon Scalable 4th)", "Intel 7")),
            0xCF => Some(("Emerald Rapids (Xeon Scalable 5th)", "Intel 7")),
            0xAD => Some(("Granite Rapids-SP (Xeon 6)", "Intel 3")),

            // --- Meteor Lake (Intel 4) ---
            0xAA => Some(("Meteor Lake", "Intel 4")),

            // --- Arrow Lake / Lunar Lake (Core Ultra Series 2, TSMC N3B) ---
            0xC6 => Some(("Arrow Lake-S", "TSMC N3B")),
            0xC5 => Some(("Arrow Lake-H/HX", "TSMC N3B")),
            0xB5 => Some(("Arrow Lake-U", "TSMC N3B")),
            0xBD => Some(("Lunar Lake", "TSMC N3B")),

            // --- Low-power Atom lines (common in entry-level hosts) ---
            0x37 => Some(("Bay Trail (Silvermont)", "22 nm")),
            0x4C => Some(("Braswell/Cherry Trail (Airmont)", "14 nm")),
            0x5C => Some(("Apollo Lake (Goldmont)", "14 nm")),
            0x5F => Some(("Denverton (Goldmont)", "14 nm")),
            0x7A => Some(("Gemini Lake (Goldmont Plus)", "14 nm")),
            0x8A => Some(("Lakefield", "10 nm")),
            0x96 => Some(("Elkhart Lake (Tremont)", "10 nm")),
            0x9C => Some(("Jasper Lake (Tremont)", "10 nm")),
            0x66 => Some(("Cannon Lake", "14++ nm")),

            _ => None,
        },

        // =====================================================================
        // AMD — K8 .. Zen 5, keyed by (display family, display model)
        // =====================================================================
        (CpuVendor::Amd, f, m) => match (f, m) {
            // --- Family 0x0F (15): K8 Athlon 64 / Opteron ---
            (0x0F, 0x04) => Some(("Athlon 64 (ClawHammer)", "130 nm")),
            (0x0F, 0x0C) => Some(("Athlon 64 (Newcastle/Paris)", "130 nm")),
            (0x0F, 0x1F) => Some(("Athlon 64 (Winchester)", "90 nm")),
            (0x0F, 0x1C) => Some(("Sempron 64 (Palermo/Sonora)", "90 nm")),
            (0x0F, 0x27) => Some(("Athlon 64 (San Diego)", "90 nm")),
            (0x0F, 0x2C) => Some(("Athlon 64 (Venice)", "90 nm")),
            (0x0F, 0x2F) => Some(("Athlon 64 (Venice/Palermo)", "90 nm")),
            (0x0F, 0x37) => Some(("Athlon 64 (San Diego)", "90 nm")),
            (0x0F, 0x2B) => Some(("Athlon 64 X2 (Manchester)", "90 nm")),
            (0x0F, 0x23) => Some(("Athlon 64 X2 (Toledo)", "90 nm")),
            (0x0F, 0x43) => Some(("Athlon 64 X2 (Windsor)", "90 nm")),
            (0x0F, 0x4B) => Some(("Athlon 64 X2 (Windsor)", "90 nm")),
            (0x0F, 0x6B) => Some(("Athlon 64 X2 (Brisbane)", "65 nm")),
            (0x0F, 0x4F) => Some(("Athlon 64 (Orleans/Manila)", "90 nm")),
            (0x0F, 0x5F) => Some(("Athlon 64 (Orleans/Manila)", "90 nm")),
            (0x0F, 0x68) => Some(("Turion 64 X2 (Tyler)", "65 nm")),
            (0x0F, 0x7F) => Some(("Sempron 64 (Sparta)", "65 nm")),
            (0x0F, 0x24) => Some(("Turion 64 (Lancaster)", "90 nm")),
            (0x0F, 0x48) => Some(("Turion X2 (Taylor/Trinidad)", "90 nm")),

            // --- Family 0x10 (16): K10 / K10.5 Phenom ---
            (0x10, 0x02) => Some(("Phenom X4/X3 (Agena/Toliman) / Opteron Barcelona", "65 nm")),
            (0x10, 0x04) => Some(("Phenom II (Deneb/Zosma) / Opteron Shanghai", "45 nm")),
            (0x10, 0x05) => Some(("Phenom II / Athlon II (Deneb/Rana/Propus)", "45 nm")),
            (0x10, 0x06) => Some(("Athlon II (Regor/Champlain)", "45 nm")),
            (0x10, 0x08) => Some(("Opteron (Istanbul/Lisbon)", "45 nm")),
            (0x10, 0x09) => Some(("Opteron (Magny-Cours)", "45 nm")),
            (0x10, 0x0A) => Some(("Phenom II X6 (Thuban)", "45 nm")),

            // --- Family 0x11 (17): Griffin mobile K8 ---
            (0x11, 0x03) => Some(("Turion X2 (Griffin)", "65 nm")),

            // --- Family 0x12 (18): Llano APU ---
            (0x12, 0x01) => Some(("Llano (A/E-Series)", "GF 32nm SOI")),

            // --- Family 0x14 (20): Bobcat (upstream keys by family only) ---
            (0x14, _) => Some(("Bobcat (Ontario/Zacate)", "TSMC 40nm")),

            // --- Family 0x15 (21): Bulldozer -> Excavator ---
            (0x15, 0x00) => Some(("FX (Zambezi, Bulldozer)", "GF 32nm SOI")),
            (0x15, 0x01) => Some(("FX (Zambezi) / Opteron (Interlagos)", "GF 32nm SOI")),
            (0x15, 0x02) => Some(("FX (Vishera) / Opteron (Abu Dhabi)", "GF 32nm SOI")),
            (0x15, 0x10) => Some(("Trinity (Piledriver)", "GF 32nm SOI")),
            (0x15, 0x13) => Some(("Richland (Piledriver)", "GF 32nm SOI")),
            (0x15, 0x30) => Some(("Kaveri (Steamroller)", "TSMC 28nm")),
            (0x15, 0x38) => Some(("Godavari (Steamroller)", "TSMC 28nm")),
            (0x15, 0x60) => Some(("Carrizo (Excavator)", "GF 28nm SOI")),
            (0x15, 0x65) => Some(("Bristol Ridge (Excavator)", "GF 28nm SOI")),
            (0x15, 0x70) => Some(("Stoney Ridge (Excavator)", "GF 28nm SOI")),

            // --- Family 0x16 (22): Jaguar / Puma ---
            (0x16, 0x00) => Some(("Kabini (Jaguar)", "TSMC 28nm")),
            (0x16, 0x30) => Some(("Mullins/Beema (Puma)", "GF 28nm SOI")),

            // --- Family 0x17 (23): Zen / Zen+ / Zen 2 ---
            (0x17, 0x01) => Some(("Zen (Summit Ridge/Naples)", "GF 14nm")),
            (0x17, 0x08) => Some(("Zen+ (Pinnacle Ridge/Colfax)", "GF 12nm")),
            (0x17, 0x11) => Some(("Zen (Raven Ridge)", "GF 14nm")),
            (0x17, 0x18) => Some(("Zen+ (Picasso)", "GF 12nm")),
            (0x17, 0x20) => Some(("Zen (Dali)", "GF 14nm")),
            (0x17, 0x31) => Some(("Zen 2 (Rome/Castle Peak)", "TSMC N7 (cores)")),
            (0x17, 0x71) => Some(("Zen 2 (Matisse)", "TSMC N7 (cores)")),
            (0x17, 0x60) => Some(("Zen 2 (Renoir)", "TSMC N7")),
            (0x17, 0x68) => Some(("Zen 2 (Lucienne)", "TSMC N7")),
            (0x17, 0x47) => Some(("Zen 2 (4700S Desktop Kit)", "TSMC N7")),
            (0x17, 0x84) => Some(("Zen 2 (4800S Desktop Kit)", "TSMC N7")),
            (0x17, 0x90) | (0x17, 0x91) => Some(("Zen 2 (Van Gogh)", "TSMC N7")),
            (0x17, 0xA0) => Some(("Zen 2 (Mendocino)", "TSMC N6")),

            // --- Family 0x19 (25): Zen 3 / Zen 3+ / Zen 4 ---
            (0x19, 0x01) => Some(("Zen 3 (Milan)", "TSMC N7")),
            (0x19, 0x08) => Some(("Zen 3 (Chagall)", "TSMC N7")),
            (0x19, 0x21) => Some(("Zen 3 (Vermeer)", "TSMC N7")),
            (0x19, 0x50) => Some(("Zen 3 (Cezanne/Barcelo)", "TSMC N7")),
            (0x19, 0x44) => Some(("Zen 3+ (Rembrandt)", "TSMC N6")),
            (0x19, 0x11) => Some(("Zen 4 (Genoa)", "TSMC N5")),
            (0x19, 0x18) => Some(("Zen 4 (Storm Peak)", "TSMC N5")),
            // 0x61 covers Raphael desktop and the Dragon Range mobile HX
            // parts (stepping-split upstream).
            (0x19, 0x61) => Some(("Zen 4 (Raphael/Dragon Range)", "TSMC N5 (cores)")),
            (0x19, 0x74) => Some(("Zen 4 (Phoenix)", "TSMC N4")),
            (0x19, 0x75) => Some(("Zen 4 (Hawk Point/Phoenix 2)", "TSMC N4")),

            // --- Family 0x1A (26): Zen 5 ---
            (0x1A, 0x02) => Some(("Zen 5 (Turin)", "TSMC N4X")),
            (0x1A, 0x11) => Some(("Zen 5c (Turin Dense)", "TSMC N3E")),
            (0x1A, 0x08) => Some(("Zen 5 (Shimada Peak)", "TSMC N4")),
            (0x1A, 0x44) => Some(("Zen 5 (Granite Ridge)", "TSMC N4")),
            (0x1A, 0x24) => Some(("Zen 5 (Strix Point)", "TSMC N4P")),
            (0x1A, 0x60) => Some(("Zen 5 (Krackan Point)", "TSMC N4P")),
            (0x1A, 0x70) => Some(("Zen 5 (Strix Halo)", "TSMC N4P")),

            _ => None,
        },

        _ => None,
    }
}

#[cfg(test)]
#[path = "../../tests/headless/core_core_cpu_codename_tests.rs"]
mod tests;
