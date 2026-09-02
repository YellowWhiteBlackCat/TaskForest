//! The fixture-side CPU topology generator: typed cluster specs whose
//! derivations (per-logical-CPU types plus utilization/frequency/temperature
//! seeds) keep every demo vector one derivation away from the declared
//! topology, so GPUI, Iced, TUI and Bevy all read the same elastic fields and
//! none of them hardcodes a shape.

/// One CPU cluster: physical cores of ONE class sharing one SMT shape.
/// Free composition across any number of clusters is what makes the model
/// cover the market: homogeneous AMD/server parts are ONE cluster with SMT
/// on every core, Intel P+E is two, Intel P+E+LP-E is three, Apple/ARM
/// big.LITTLE and Snapdragon-style 1X+5P+2LP shapes are two–three, and a
/// fourth cluster costs one more entry — never a new model.
#[derive(Clone, Copy, Debug)]
pub struct CpuClusterSpec {
    /// The class every core in this cluster reports.
    pub kind: taskmanager_core::core::hardware::CpuType,
    /// Physical cores in this cluster.
    pub physical_cores: usize,
    /// Logical CPUs per physical core: 1 = no SMT, 2 = SMT/Hyper-Threading.
    /// ANY cluster may carry SMT — the model does not bake Intel's
    /// "E-cores have no SMT" marketplace accident into a law (AMD Zen runs
    /// SMT on every core).
    pub threads_per_core: usize,
}

/// A CPU topology spec: an ordered cluster list. This is the fixture-side
/// GENERATOR only. The core-ized truth every frontend consumes is its
/// DERIVATION — the per-logical-CPU `hardware.cpu_types` array plus the
/// declared counts (`cpu_types`/`cpu_cores`/`physical_cores`/
/// `logical_cores`) — so GPUI, Iced, TUI and Bevy all read the same elastic
/// fields and none of them hardcodes a topology.
#[derive(Clone, Debug)]
pub struct CpuTopologySpec {
    pub clusters: Vec<CpuClusterSpec>,
}

impl CpuTopologySpec {
    /// Physical cores summed over all clusters.
    pub fn physical_cores(&self) -> usize {
        self.clusters
            .iter()
            .map(|cluster| cluster.physical_cores)
            .sum()
    }

    /// Logical CPUs summed over all clusters
    /// (Σ physical × threads-per-core).
    pub fn logical_cores(&self) -> usize {
        self.clusters
            .iter()
            .map(|cluster| cluster.physical_cores * cluster.threads_per_core)
            .sum()
    }

    /// Per-logical-CPU type in cluster order (the order every consumer —
    /// grid grouping, captions — must preserve).
    pub fn cpu_types(&self) -> Vec<taskmanager_core::core::hardware::CpuType> {
        self.clusters
            .iter()
            .flat_map(|cluster| {
                tiled(
                    &[cluster.kind],
                    cluster.physical_cores * cluster.threads_per_core,
                )
            })
            .collect()
    }

    /// Per-logical-CPU utilization seed: Performance clusters run busy,
    /// Efficient clusters moderate, LowPower clusters near idle, Unknown
    /// falls back to the moderate band. Tiling continues across same-kind
    /// clusters (the offset walks forward), keeping values deterministic and
    /// plausible for any composition.
    pub fn core_usage(&self) -> Vec<f32> {
        let mut values = Vec::with_capacity(self.logical_cores());
        for cluster in &self.clusters {
            let (pattern, offset) = (
                usage_pattern(cluster.kind),
                self.painted_logical_of_kind(cluster.kind),
            );
            values.extend(tiled_offset(pattern, cluster.logical_cpus(), offset));
        }
        values
    }

    /// Per-logical-CPU clock seed (MHz): Performance clusters boost highest,
    /// LowPower clusters sit at their floor.
    pub fn frequencies_mhz(&self) -> Vec<u64> {
        let mut values = Vec::with_capacity(self.logical_cores());
        for cluster in &self.clusters {
            let (pattern, offset) = (
                frequency_pattern(cluster.kind),
                self.painted_logical_of_kind(cluster.kind),
            );
            values.extend(tiled_offset(pattern, cluster.logical_cpus(), offset));
        }
        values
    }

    /// Per-logical-CPU temperature seed (°C), tracking the utilization shape.
    pub fn temperatures_c(&self) -> Vec<f32> {
        let mut values = Vec::with_capacity(self.logical_cores());
        for cluster in &self.clusters {
            let (pattern, offset) = (
                temperature_pattern(cluster.kind),
                self.painted_logical_of_kind(cluster.kind),
            );
            values.extend(tiled_offset(pattern, cluster.logical_cpus(), offset));
        }
        values
    }

    /// How many logical CPUs of `kind` precede this cluster — the tiling
    /// offset so same-kind clusters continue the pattern instead of
    /// restarting it.
    fn painted_logical_of_kind(&self, kind: taskmanager_core::core::hardware::CpuType) -> usize {
        self.clusters
            .iter()
            .take_while(|cluster| cluster.kind != kind)
            .map(|cluster| cluster.logical_cpus())
            .sum()
    }
}

impl CpuClusterSpec {
    /// Logical CPUs this cluster paints.
    pub const fn logical_cpus(&self) -> usize {
        self.physical_cores * self.threads_per_core
    }
}

/// The demo host profile: an Ultra 7 358H-class hybrid part — 6 P-cores with
/// SMT (12 logical) + 8 E-cores + 2 LP-E-cores = 16 physical / 22 logical.
/// One profile INSTANCE of the cluster-list generator; the shape itself is
/// not baked into any model, and any other market topology (homogeneous
/// AMD/server with SMT on every core, Snapdragon-style 1X+5P+2LP,
/// Apple-style big.LITTLE, a fourth cluster…) is one literal swap away.
pub fn demo_cpu_topology() -> CpuTopologySpec {
    use taskmanager_core::core::hardware::CpuType;
    CpuTopologySpec {
        clusters: vec![
            CpuClusterSpec {
                kind: CpuType::Performance,
                physical_cores: 6,
                threads_per_core: 2,
            },
            CpuClusterSpec {
                kind: CpuType::Efficient,
                physical_cores: 8,
                threads_per_core: 1,
            },
            CpuClusterSpec {
                kind: CpuType::LowPower,
                physical_cores: 2,
                threads_per_core: 1,
            },
        ],
    }
}

/// Per-kind base patterns. A cluster of size *n* tiles the *n* entries of its
/// kind's pattern starting at the kind's running offset, so the demo profile
/// reproduces the original hand-written values exactly while any other
/// topology stays deterministic and plausible. Patterns live per KIND (not
/// per full vector) precisely so topology changes never require re-writing
/// literal vectors.
fn usage_pattern(kind: taskmanager_core::core::hardware::CpuType) -> &'static [f32] {
    use taskmanager_core::core::hardware::CpuType;
    match kind {
        CpuType::Performance => &[
            52.0, 41.0, 34.0, 22.0, 57.5, 33.0, 48.5, 39.0, 44.5, 28.0, 61.5, 36.0,
        ],
        CpuType::Efficient => &[18.0, 25.5, 12.0, 31.0, 9.5, 22.5, 15.0, 27.0],
        CpuType::LowPower => &[4.5, 7.0],
        CpuType::Unknown => &[21.0, 33.0],
    }
}

fn frequency_pattern(kind: taskmanager_core::core::hardware::CpuType) -> &'static [u64] {
    use taskmanager_core::core::hardware::CpuType;
    match kind {
        CpuType::Performance => &[
            4_820, 4_760, 4_910, 4_640, 4_750, 4_690, 4_880, 4_710, 4_800, 4_655, 4_940, 4_725,
        ],
        CpuType::Efficient => &[3_380, 3_450, 3_360, 3_420, 3_310, 3_470, 3_390, 3_440],
        CpuType::LowPower => &[1_250, 1_180],
        CpuType::Unknown => &[2_800, 2_650],
    }
}

fn temperature_pattern(kind: taskmanager_core::core::hardware::CpuType) -> &'static [f32] {
    use taskmanager_core::core::hardware::CpuType;
    match kind {
        CpuType::Performance => &[
            58.0, 56.5, 61.0, 54.0, 57.5, 55.5, 59.5, 56.0, 58.5, 54.5, 62.0, 57.0,
        ],
        CpuType::Efficient => &[49.0, 50.5, 48.0, 51.0, 47.5, 50.0, 48.5, 49.5],
        CpuType::LowPower => &[43.0, 42.5],
        CpuType::Unknown => &[47.0, 48.0],
    }
}

fn tiled_offset<T: Copy>(pattern: &[T], len: usize, offset: usize) -> Vec<T> {
    (0..len)
        .map(|index| pattern[(index + offset) % pattern.len()])
        .collect()
}

fn tiled<T: Copy>(pattern: &[T], len: usize) -> Vec<T> {
    tiled_offset(pattern, len, 0)
}

/// Every per-core seed vector in the demo snapshot derives from
/// [`demo_cpu_topology`], so the fixture can never contradict its own
/// topology declaration: vector lengths, `cpu_types` and the declared
/// physical/logical counts are one derivation apart.
pub(super) fn core_usage_seed() -> Vec<f32> {
    demo_cpu_topology().core_usage()
}

pub(super) fn per_core_frequency_seed() -> Vec<u64> {
    demo_cpu_topology().frequencies_mhz()
}

pub(super) fn per_core_temperature_seed() -> Vec<f32> {
    demo_cpu_topology().temperatures_c()
}

pub(super) fn cpu_types_seed() -> Vec<taskmanager_core::core::hardware::CpuType> {
    demo_cpu_topology().cpu_types()
}

#[cfg(test)]
#[path = "../../tests/headless/cpu_topology_tests.rs"]
mod topology_tests;
