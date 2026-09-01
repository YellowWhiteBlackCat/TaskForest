//! Serializable user preferences for Task Manager.
//!
//! Stores the active theme (skin + color mode + high-contrast), the sidebar
//! device toggles, Apps-page presentation preferences, the collector refresh
//! interval, the last-selected top-level page, the startup-page policy
//! (remember last / fixed page), and the animation (motion) preference. This
//! module owns only the version-tolerant data model; native path selection
//! and filesystem persistence belong to outer layers.
//!
//! `skin` / `mode` are stored as the human-readable label strings produced by
//! the theme engine (`Skin::label` / `LightDark::label`). On the `core` side
//! we only carry opaque `String`s — the string→enum mapping lives in
//! `gpui_app::root`, which depends on `core` (not the reverse). An empty
//! `skin`/`mode` (the [`Default`]) means "no preference recorded yet"; the
//! load path then keeps whatever native appearance mapping produced, so a first
//! launch behaves identically to before this module existed.

use serde::{Deserialize, Serialize};

/// Platform-neutral canonical state for one user-created process view preset.
///
/// The running product has one category-first hierarchy, so no retired view
/// mode is stored here. Schema-v1 mode tokens exist only in the private serde
/// DTO below; incompatible preset records are filtered while the rest of a
/// configuration remains readable.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProcessViewPresetConfig {
    pub name: String,
    pub filter: String,
    pub sort: String,
    pub sort_asc: bool,
    pub hidden_columns: Vec<String>,
}

impl ProcessViewPresetConfig {
    #[must_use]
    pub fn new(
        name: String,
        filter: String,
        sort: String,
        sort_asc: bool,
        hidden_columns: Vec<String>,
    ) -> Self {
        Self {
            name,
            filter,
            sort,
            sort_asc,
            hidden_columns,
        }
    }

    fn from_wire(wire: ProcessViewPresetReadWire) -> Option<Self> {
        wire.mode
            .as_deref()
            .is_none_or(is_process_view_mode_import_token)
            .then_some(Self {
                name: wire.name,
                filter: wire.filter,
                sort: wire.sort,
                sort_asc: wire.sort_asc,
                hidden_columns: wire.hidden_columns,
            })
    }
}

#[derive(Deserialize)]
struct ProcessViewPresetReadWire {
    #[serde(default)]
    name: String,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    filter: String,
    #[serde(default)]
    sort: String,
    #[serde(default)]
    sort_asc: bool,
    #[serde(default)]
    hidden_columns: Vec<String>,
}

/// Canonical write shape. Retired view-mode state is accepted only by the
/// read DTO above; current writers never publish it back to disk.
#[derive(Serialize)]
struct ProcessViewPresetWriteWire<'a> {
    name: &'a str,
    filter: &'a str,
    sort: &'a str,
    sort_asc: bool,
    hidden_columns: &'a [String],
}

impl Serialize for ProcessViewPresetConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        ProcessViewPresetWriteWire {
            name: &self.name,
            filter: &self.filter,
            sort: &self.sort,
            sort_asc: self.sort_asc,
            hidden_columns: &self.hidden_columns,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ProcessViewPresetConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Self::from_wire(ProcessViewPresetReadWire::deserialize(deserializer)?)
            .ok_or_else(|| serde::de::Error::custom("unsupported schema-v1 process view mode"))
    }
}

fn deserialize_process_view_presets<'de, D>(
    deserializer: D,
) -> Result<Vec<ProcessViewPresetConfig>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Vec::<ProcessViewPresetReadWire>::deserialize(deserializer)?
        .into_iter()
        .filter_map(ProcessViewPresetConfig::from_wire)
        .collect())
}

mod saved_view_transfer;
pub use saved_view_transfer::{
    MAX_PRESET_NAME_CHARS, MAX_TRANSFER_PRESETS, SAVED_VIEW_TRANSFER_FORMAT,
    SAVED_VIEW_TRANSFER_VERSION, SavedViewIdAllocation, SavedViewImportNames,
    SavedViewTransferError, allocate_saved_view_ids, export_saved_views_document,
    import_saved_views_document, resolve_saved_view_import_names, saved_view_name_is_portable,
    unique_saved_view_name,
};

/// Platform-neutral serialized form of one user-resized process column width.
///
/// The `column` token is opaque to `core` (the same `"CPU"` / `"Memory"` /
/// `"PID"` strings [`ProcessViewPresetConfig`] round-trips for its `sort` /
/// `hidden_columns` fields); the GPUI layer owns the token↔`SortCol` mapping
/// and the `f32`↔`Pixels` conversion. Storing widths as plain `f32` keeps
/// `core` free of the gpui `Pixels` type (core must not depend on gpui).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ColumnWidthConfig {
    /// Opaque column token (e.g. `"Memory"`, `"CPU"`). Unknown / non-resizable
    /// tokens are dropped on load, never a panic.
    pub column: String,
    /// Resized column width in device pixels. Non-finite / non-positive values
    /// are dropped on load; oversized values clamp to the column-width ceiling.
    pub width: f32,
}

/// Platform-neutral persisted visibility override for one Performance sidebar
/// device. The UI owns the runtime device-key vocabulary; core only preserves
/// the key and the explicit show/hide decision across config migrations.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SidebarDeviceOverrideConfig {
    /// Stable UI key such as `disk:<device-id>` or `network:<device-id>`.
    pub device: String,
    /// Explicit per-device decision. Category visibility remains the fallback
    /// when no override exists, matching Mission Center's precedence rules.
    pub visible: bool,
}

/// Persisted user preferences. Round-trips through `config.json` via serde.
///
/// Field set mirrors the user-tunable surface in the Settings dialog plus the
/// last-selected top-level page; adding a preference is "add a field, give it
/// a [`Default`] value, read/write it in `root.rs`".
/// The container intentionally remains tolerant of unknown fields: historical
/// root `process_view_mode` keys are consumed and discarded by this serde
/// boundary because the canonical domain has no corresponding state.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
/// Container-level default: any field a config file omits falls back to
/// [`Config::default`] — a minimal file naming one preference (the capture
/// harness writes `{"history_persistence": true}`) must parse, not take the
/// whole store down to defaults.
#[serde(default)]
pub struct Config {
    /// Active skin token: `"GNOME"` / `"KDE"` / `"Windows"` / `"macOS"`
    /// (the `Skin::label()` strings). Parsed back case-insensitively on load;
    /// an empty/unknown value falls back to the host-detected skin.
    pub skin: String,
    /// Color-scheme token: `"Light"` / `"Dark"` / `"EyeForest"` / `"System"`. `"System"`
    /// follows the native desktop appearance; an empty token is the legacy
    /// first-launch sentinel and is treated the same way. The GPUI layer owns
    /// the token→resolved-mode mapping, so core remains toolkit-neutral.
    pub mode: String,
    /// High-contrast accessibility axis.
    pub hc: bool,
    /// Font preference tokens: `""` = system font (per skin), otherwise the
    /// family name of a bundled face (`"MiSans VF"` / `"Roboto Mono"`).
    /// Opaque strings on the core side; `gpui_app` maps them to its
    /// `FontChoice` enums (unknown values fall back to
    /// system). Empty strings match the `skin`/`mode` sentinel pattern.
    #[serde(default)]
    pub ui_font: String,
    #[serde(default)]
    pub mono_font: String,
    /// Table row-density token: `"Comfortable"` / `"Compact"` (the
    /// `RowDensity` labels; empty = no recorded preference → the built-in
    /// comfortable geometry). Same opaque-token split as `skin`/`mode`.
    #[serde(default)]
    pub density: String,
    /// Product-wide desktop UI-size token: `"Small"` / `"Standard"` /
    /// `"Large"`. Empty/unknown values resolve to the readability-first
    /// Standard profile at each desktop renderer boundary. This is separate
    /// from row density and from compositor DPI scaling.
    #[serde(default)]
    pub ui_size: String,
    /// Text-rendering mode token: `""` = platform default, `"subpixel"`,
    /// `"grayscale"` (see [`TEXT_RENDERING_PLATFORM_DEFAULT`] et al.). Opaque
    /// on the core side; `gpui_app` maps it to gpui's text-rendering mode
    /// (empty/unknown → PlatformDefault), the same split as `skin`/`mode`.
    #[serde(default)]
    pub text_rendering: String,
    /// Animation (motion) preference token: `"normal"` (the default) /
    /// `"reduced"` / `"none"` (see [`MOTION_NORMAL`] et al.). Opaque on the
    /// core side; each desktop frontend maps it onto the shared theme's
    /// `MotionPolicy` (unknown tokens degrade to Normal, never a panic), the
    /// same split as `skin`/`mode`. A missing field (an old config file)
    /// keeps the full-motion default.
    #[serde(default = "default_motion")]
    pub motion: String,
    /// Window-frame policy token: `""` = follow the compositor negotiation
    /// ([`WINDOW_DECORATIONS_SYSTEM`]), `"native"` = prefer the OS-drawn
    /// titlebar ([`WINDOW_DECORATIONS_NATIVE`]), `"custom"` = prefer the
    /// app-drawn titlebar with transparent rounded corners
    /// ([`WINDOW_DECORATIONS_CUSTOM`]). Opaque on the core side; the frontend
    /// maps the token to its toolkit's decoration request (unknown values
    /// fail closed to System), the same split as `skin`/`mode`. A missing
    /// field (an old config file) keeps the negotiation default.
    #[serde(default)]
    pub window_decorations: String,
    /// Sidebar device-category toggles (Performance page).
    pub show_cpu: bool,
    pub show_memory: bool,
    pub show_disks: bool,
    /// Master network visibility toggle. The following five fields retain
    /// the upstream Mission Center category split and default to visible when
    /// loading a pre-category config file.
    pub show_network: bool,
    #[serde(default = "default_true")]
    pub show_network_wired: bool,
    #[serde(default = "default_true")]
    pub show_network_wireless: bool,
    #[serde(default = "default_true")]
    pub show_network_vpn: bool,
    #[serde(default = "default_true")]
    pub show_network_virtual: bool,
    #[serde(default = "default_true")]
    pub show_network_other: bool,
    pub show_gpus: bool,
    /// Performance-page memory unit preference: `true` = bytes, `false` = bits.
    /// Mission Center defaults to bytes for memory.
    #[serde(default = "default_true")]
    pub memory_use_bytes: bool,
    /// Performance-page memory base preference: `true` = base 2, `false` = base 10.
    #[serde(default = "default_true")]
    pub memory_use_base2: bool,
    /// Performance-page drive unit preference: `true` = bytes, `false` = bits.
    #[serde(default = "default_true")]
    pub drive_use_bytes: bool,
    /// Performance-page drive base preference: `true` = base 2, `false` = base 10.
    #[serde(default = "default_true")]
    pub drive_use_base2: bool,
    /// Performance-page network unit preference: `true` = bytes, `false` = bits.
    /// Mission Center defaults to bits for network transfer rates.
    #[serde(default)]
    pub network_use_bytes: bool,
    /// Performance-page network base preference: `true` = base 2, `false` = base 10.
    #[serde(default)]
    pub network_use_base2: bool,
    /// Number of samples displayed in each Performance graph. The GPUI layer
    /// clamps this opaque preference to Mission Center's 10..=600 range.
    #[serde(default = "default_graph_data_points")]
    pub graph_data_points: u32,
    /// Animate the Performance graph refresh transition.
    #[serde(default)]
    pub sliding_graphs: bool,
    /// Use the observed network peak (`true`) or the interface link speed (`false`)
    /// as the network graph scale.
    #[serde(default = "default_true")]
    pub network_dynamic_scaling: bool,
    /// Persisted order of concrete Performance sidebar device keys. Missing or
    /// unknown keys are ignored by the GPUI projection; newly discovered
    /// devices retain the built-in order and are appended deterministically.
    #[serde(default)]
    pub sidebar_order: Vec<String>,
    /// Explicit per-device visibility overrides. Category switches are still
    /// the fallback, while a known override wins for its concrete device.
    #[serde(default)]
    pub sidebar_device_overrides: Vec<SidebarDeviceOverrideConfig>,
    /// Dim current zero-valued resource cells on the Apps page. `None`/missing
    /// values keep their existing unavailable-value rendering and are not
    /// treated as zero.
    #[serde(default)]
    pub gray_zero_values: bool,
    /// Collector refresh interval in milliseconds.
    pub refresh_ms: u64,
    /// Last-selected top-level page token (e.g. `"performance"`, `"apps"`).
    pub last_page: String,
    /// Startup-page policy token: `""` = remember the last page
    /// ([`STARTUP_PAGE_REMEMBER`]), `"performance"` / `"apps"` = always open
    /// that fixed page, overriding the recorded [`Config::last_page`]. Opaque
    /// on the core side; `gpui_app` maps it to the page vocabulary (unknown
    /// values fall back to remember-last). The empty string matches the
    /// `skin`/`mode` sentinel pattern, so a first launch (and an old config
    /// file written before this field existed) behaves exactly as before.
    #[serde(default)]
    pub startup_page: String,
    /// Deliver desktop notifications for fired alerts (BN-07, extension
    /// capability `alerts.notify`). Default off: delivery is an explicit
    /// opt-in, matching the repository's privacy discipline (no ambient
    /// desktop noise without user intent).
    #[serde(default)]
    pub notify_enabled: bool,
    /// Optional quiet hours as `(start, end)` minutes after midnight;
    /// `start >= end` spans midnight (e.g. 22:00–07:00 = `(1320, 420)`).
    /// `None` = no quiet hours. Consumed by the pure [`crate::core::alerts::NotificationGate`]
    /// policy; core stores the opaque tuple, frontends own the input UI.
    #[serde(default)]
    pub notify_quiet_hours: Option<(u16, u16)>,
    // ── Persisted process-list UI state ───────────────────────────────────
    // Stored as OPAQUE string tokens on the core side — the token↔enum mapping
    // lives in `gpui_app::root` (which depends on `core`, not the reverse),
    // exactly how `skin`/`mode` are handled above. An empty/unknown value means
    // "no recorded preference" and the view falls back to its built-in default.
    /// Active sort column token (e.g. `"CPU"`, `"Memory"`, `"Name"`, `"PID"` —
    /// the view owns the exact spelling). Empty = "no recorded preference" →
    /// the view uses its built-in default sort (CPU descending), matching the
    /// empty-string sentinel pattern used by `skin`/`mode`.
    #[serde(default)]
    pub process_sort_col: String,
    /// Sort direction recorded alongside [`Config::process_sort_col`]: `true` =
    /// ascending, `false` = descending. `false` matches the Task Manager
    /// CPU-descending default.
    #[serde(default)]
    pub process_sort_asc: bool,
    /// Process columns the user has HIDDEN, as opaque column tokens (the view
    /// owns the token↔column mapping). Empty = all columns visible (the
    /// pre-persistence default). Storing the HIDDEN set rather than the visible
    /// one means a column added in a future version defaults to visible, so an
    /// upgrade never silently hides a new column the user has never seen.
    #[serde(default)]
    pub process_hidden_columns: Vec<String>,
    /// Presence bit for [`Config::process_hidden_columns`]. Old payloads with
    /// an absent/empty list keep the product's built-in column set; current
    /// writers set this before saving so an explicit empty list can mean
    /// "show every column" rather than "no preference recorded".
    #[serde(default)]
    pub process_hidden_columns_configured: bool,
    /// User-resized process column widths, as opaque column token → pixel width
    /// (see [`ColumnWidthConfig`]). Empty = every column at its built-in
    /// `default_width` (the pre-persistence byte-identical layout), so a first
    /// launch and an old config file written before this field existed behave
    /// exactly as before. The `Name` (identity / flex-grow) column is
    /// non-resizable and never appears here. Mirrors the
    /// `process_hidden_columns` token split: core carries opaque strings, the
    /// GPUI layer owns the token↔`SortCol` mapping + the `f32`↔`Pixels`
    /// conversion.
    #[serde(default)]
    pub process_col_widths: Vec<ColumnWidthConfig>,
    /// User-resized devices-sidebar width in device pixels. The serde default
    /// is the pre-persistence 260px so a first launch and an old config file
    /// behave exactly as before; the GPUI load path clamps to `[200, 460]`.
    #[serde(default = "default_sidebar_width")]
    pub sidebar_width: f32,
    /// User-created Dashboard process presets only. Built-in presets are
    /// reconstructed by the UI and must never be duplicated in this list.
    #[serde(default, deserialize_with = "deserialize_process_view_presets")]
    pub saved_process_views: Vec<ProcessViewPresetConfig>,
    /// Persisted UI-language token. Known values are `"en"` / `"zh"` (the
    /// `locales/` bundle stems); core stores an opaque validated token the
    /// same way as `skin`/`mode` — the token→i18n-bundle mapping and any
    /// validation belong to the consumers (core cannot depend on the
    /// application layer's i18n type). `None` = no recorded preference (also
    /// the serde default for an old config file), so each frontend keeps its
    /// own zh→en→key fallback chain until the user picks a language.
    #[serde(default)]
    pub language: Option<String>,
    /// Opt-in telemetry-history persistence (roadmap #4, R1 / ADR-028).
    /// `false` — the privacy default — writes NOTHING to disk: no store is
    /// constructed, no sink is attached, no history directory is touched.
    /// `true` enables JSONL series persistence under the platform's user-data
    /// history directory with the 7-day / 500 MB retention contract.
    #[serde(default)]
    pub history_persistence: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            // Empty => "no recorded preference": the load path keeps the
            // host-detected skin/mode rather than forcing a value. After the
            // first save these are populated from the live theme labels.
            skin: String::new(),
            mode: String::new(),
            hc: false,
            ui_font: String::new(),
            mono_font: String::new(),
            // Empty token => comfortable table geometry (pre-persistence look).
            density: String::new(),
            // Empty token => the new readability-first Standard UI profile.
            ui_size: String::new(),
            // Empty token => platform default (the OS/GPU driver's choice).
            text_rendering: TEXT_RENDERING_PLATFORM_DEFAULT.to_string(),
            // Full-motion default: an old config file (and a cold start)
            // keeps the pre-preference animated behavior.
            motion: MOTION_NORMAL.to_string(),
            // Compositor-negotiation default: an old config file (and a cold
            // start) keep the request-native + follow-the-grant behavior.
            window_decorations: WINDOW_DECORATIONS_SYSTEM.to_string(),
            show_cpu: true,
            show_memory: true,
            show_disks: true,
            show_network: true,
            show_network_wired: true,
            show_network_wireless: true,
            show_network_vpn: true,
            show_network_virtual: true,
            show_network_other: true,
            show_gpus: true,
            memory_use_bytes: true,
            memory_use_base2: true,
            drive_use_bytes: true,
            drive_use_base2: true,
            network_use_bytes: false,
            network_use_base2: false,
            graph_data_points: default_graph_data_points(),
            sliding_graphs: false,
            network_dynamic_scaling: true,
            sidebar_order: Vec::new(),
            sidebar_device_overrides: Vec::new(),
            // Preserve the pre-preference rendering: zero-valued resource
            // cells use their metric color until the user opts in.
            gray_zero_values: false,
            refresh_ms: 1000,
            // Desktop notifications default OFF: explicit opt-in (privacy
            // discipline, BN-07).
            notify_enabled: false,
            notify_quiet_hours: None,
            last_page: PAGE_PERFORMANCE.to_string(),
            // Empty token => remember the last page (pre-persistence behavior).
            startup_page: STARTUP_PAGE_REMEMBER.to_string(),
            // Process-list UI state: no recorded preference → the canonical
            // category-first process tree. Legacy tokens remain accepted at
            // frontend migration boundaries but are never the new default.
            process_sort_col: String::new(),
            process_sort_asc: false,
            process_hidden_columns: Vec::new(),
            process_hidden_columns_configured: false,
            // Empty = no resized columns → every column at its built-in default
            // width (the pre-persistence layout).
            process_col_widths: Vec::new(),
            // Pre-persistence sidebar width (260px) — byte-identical layout
            // before the user drags the edge.
            sidebar_width: default_sidebar_width(),
            saved_process_views: Vec::new(),
            // No recorded language preference: frontends apply their own
            // fallback chain until the user picks one (G-22).
            language: None,
            // History persistence is strictly opt-in (privacy default OFF).
            history_persistence: false,
        }
    }
}

/// Single-source mapping between persisted [`Config`] notification fields
/// and the pure delivery policy used by [`crate::core::alerts::NotificationGate`] / the shared
/// AlertDispatcher (BN-07). Frontends never hand-map these fields.
impl Config {
    #[must_use]
    pub fn notification_policy(&self) -> crate::core::alerts::NotificationPolicy {
        crate::core::alerts::NotificationPolicy {
            enabled: self.notify_enabled,
            cooldown_ms: crate::core::alerts::NotificationPolicy::default().cooldown_ms,
            quiet_hours: self.notify_quiet_hours.map(|(start, end)| {
                crate::core::alerts::QuietHours {
                    start_minutes: start,
                    end_minutes: end,
                }
            }),
        }
    }

    /// Record a policy's persisted fields back into this config. Cooldown is
    /// deliberately NOT persisted (it is a delivery cadence, not a user
    /// preference); quiet hours and the opt-in switch are.
    pub fn apply_notification_policy(&mut self, policy: &crate::core::alerts::NotificationPolicy) {
        self.notify_enabled = policy.enabled;
        self.notify_quiet_hours = policy
            .quiet_hours
            .map(|hours| (hours.start_minutes, hours.end_minutes));
    }
}

/// Stable string token for the Performance top-level page (the cold-start
/// default). Mirrors the tokens `gpui_app::root` round-trips for `TopPage`.
pub const PAGE_PERFORMANCE: &str = "performance";

/// Persisted color-scheme preference tokens. `System` is a preference, not a
/// resolved light/dark palette; the executable composition edge resolves it
/// from the typed native appearance fact. The empty `Config::mode` sentinel is
/// retained for old files and maps to `System` on load.
pub const COLOR_SCHEME_SYSTEM: &str = "System";
pub const COLOR_SCHEME_LIGHT: &str = "Light";
pub const COLOR_SCHEME_DARK: &str = "Dark";
pub const COLOR_SCHEME_EYEFOREST: &str = "EyeForest";

// ── startup-page policy tokens ────────────────────────────────────────────
// Stable string tokens round-tripped via [`Config::startup_page`]. The
// `gpui_app` layer maps them to its top-page vocabulary; core only carries the
// opaque strings (same split as `skin`/`mode`). The empty token is the
// remember-last sentinel, so a first launch (and an old config file written
// before the field existed) opens the last page exactly as before.

/// Remember the last-selected top-level page (also the serde + [`Default`]
/// value for [`Config::startup_page`]).
pub const STARTUP_PAGE_REMEMBER: &str = "";
/// Always open the Performance page, overriding the recorded [`Config::last_page`].
pub const STARTUP_PAGE_PERFORMANCE: &str = PAGE_PERFORMANCE;
/// Always open the Processes (Apps) page, overriding the recorded [`Config::last_page`].
pub const STARTUP_PAGE_PROCESSES: &str = "apps";

// ── process-list view-mode tokens ──────────────────────────────────────────
// Stable wire tokens accepted at the configuration migration boundary. The
// running product has no view-mode selector: every recognized token is
// canonicalized here before a frontend sees the configuration.

/// Legacy flat-list token. Frontends accept it for migration and resolve it to
/// the canonical category-first tree.
const PROCESS_VIEW_MODE_FLAT: &str = "Flat";
/// Hierarchical process tree (parent → children).
const PROCESS_VIEW_MODE_TREE: &str = "Tree";
/// Legacy per-application grouping token; migrated to the category-first tree.
const PROCESS_VIEW_MODE_GROUP_BY_APP: &str = "GroupByApp";
/// Legacy per-type grouping token; migrated to the category-first tree.
const PROCESS_VIEW_MODE_GROUP_BY_TYPE: &str = "GroupByType";
/// Schema-v1 canonical token accepted only while reading old presets.
const PROCESS_VIEW_MODE_GROUP_BY_CATEGORY: &str = "GroupByCategory";

/// Normalize one persisted process-view token at the wire boundary.
///
/// Historical TaskForest releases wrote four alternate presentation modes.
/// They remain accepted input so upgrades never lose a user's configuration,
/// but none of those values may enter renderer state. Unknown future tokens
/// remain distinguishable (`None`) so callers can ignore unsupported records
/// rather than silently treating corrupt data as valid.
fn is_process_view_mode_import_token(token: &str) -> bool {
    matches!(
        token,
        PROCESS_VIEW_MODE_FLAT
            | PROCESS_VIEW_MODE_TREE
            | PROCESS_VIEW_MODE_GROUP_BY_APP
            | PROCESS_VIEW_MODE_GROUP_BY_TYPE
            | PROCESS_VIEW_MODE_GROUP_BY_CATEGORY
    )
}

/// Serde default for [`Config::sidebar_width`]: the pre-persistence 260px, so
/// an old config file written before the field existed loads to the original
/// sidebar width (byte-identical layout) instead of 0.
fn default_sidebar_width() -> f32 {
    260.0
}

/// Serde migration default for the per-network visibility fields introduced
/// after the single `show_network` toggle. An old config must retain the
/// previous behavior: every discovered network category remains visible.
fn default_true() -> bool {
    true
}

/// Mission Center's default Performance graph history window.
fn default_graph_data_points() -> u32 {
    60
}

// ── text-rendering mode tokens ─────────────────────────────────────────────
// Stable string tokens round-tripped via [`Config::text_rendering`]. The
// `gpui_app` layer maps them to gpui's text-rendering mode; core only carries
// the opaque strings (same split as `skin`/`mode`).

/// Platform/OS default rendering (also the serde + [`Default`] value). When
/// gpui 0.2.2 ships the mode API this maps to `PlatformDefault`.
pub const TEXT_RENDERING_PLATFORM_DEFAULT: &str = "";
/// Subpixel (LCD) antialiasing.
pub const TEXT_RENDERING_SUBPIXEL: &str = "subpixel";
/// Grayscale antialiasing.
pub const TEXT_RENDERING_GRAYSCALE: &str = "grayscale";

// ── motion preference tokens ────────────────────────────────────────────────
// Stable string tokens round-tripped via [`Config::motion`]. The desktop
// frontends map them onto the shared theme's `MotionPolicy`; core only
// carries the opaque strings (the same split as `skin`/`mode`). An unknown
// token degrades to Normal at the consumer, never a panic.

/// Full semantic motion scale (also the serde + [`Default`] value): an old
/// config file and a cold start keep the animated behavior.
pub const MOTION_NORMAL: &str = "normal";
/// Keep only brief transitions, capped at the shared 80 ms fast token.
pub const MOTION_REDUCED: &str = "reduced";
/// Skip animation and apply the final visual state immediately.
pub const MOTION_NONE: &str = "none";

/// Serde default for [`Config::motion`]: the full-motion token, so a config
/// file written before the field existed loads to the animated default
/// instead of an empty string.
fn default_motion() -> String {
    MOTION_NORMAL.to_string()
}

// ── window-frame (decoration) policy tokens ────────────────────────────────
// Stable string tokens round-tripped via [`Config::window_decorations`]. The
// desktop frontend maps them onto its toolkit's decoration request; core only
// carries the opaque strings (same split as `skin`/`mode`). An unknown token
// fails closed to System at the consumer, never a panic.

/// Follow the compositor negotiation (also the serde + [`Default`] value):
/// request native decorations, then react to what the window system actually
/// grants, falling back to the app-drawn titlebar when refused. An old config
/// file (empty token) keeps exactly this behavior.
pub const WINDOW_DECORATIONS_SYSTEM: &str = "";
/// Prefer the OS-drawn titlebar (KDE/KWin, macOS, Windows). A compositor that
/// cannot draw server-side decorations (GNOME/Mutter) will refuse; the
/// frontend then keeps the audited CSD fallback and reports that honestly.
pub const WINDOW_DECORATIONS_NATIVE: &str = "native";
/// Prefer the app-drawn titlebar with transparent rounded corners and
/// in-app minimize/maximize/close controls. Only offered on platforms whose
/// toolkit honors a client-decoration request.
pub const WINDOW_DECORATIONS_CUSTOM: &str = "custom";

// ── row-density tokens ─────────────────────────────────────────────────────
// Stable string tokens round-tripped via [`Config::density`]. The GPUI layer
// maps them to its `RowDensity` enum; core only carries the opaque strings
// (same split as `skin`/`mode`). The empty token is the sentinel for "no
// recorded preference" → the built-in comfortable geometry.

/// Standard table row geometry (also the serde + [`Default`] value).
pub const DENSITY_COMFORTABLE: &str = "Comfortable";
/// Compact table row geometry.
pub const DENSITY_COMPACT: &str = "Compact";

#[cfg(test)]
#[path = "../../tests/headless/core_core_config_tests.rs"]
mod tests;
