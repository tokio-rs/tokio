use super::TaskMeta;

use std::fmt;
use std::sync::Arc;

/// Configuration for LLC-aware scheduling on the multi-thread runtime.
///
/// LLC-aware scheduling adds one shared queue per last-level-cache
/// partition. A worker checks its current partition before considering work
/// from other LLCs on the same NUMA node, then other NUMA nodes. Tokio
/// periodically calls `current_partition` and also calls it after the worker
/// wakes, because the operating system may have migrated the worker while it
/// was asleep.
///
/// Partition identifiers must be dense integers in
/// `0..partition_count`. Returning `None`, or returning an identifier outside
/// that range, temporarily opts the worker out of LLC-aware scheduling.
/// Runtimes with fewer worker threads than partitions fall back to the
/// standard scheduler, because at least one partition would otherwise have no
/// local worker.
///
/// **Note**: This is an [unstable API][unstable]. The public API and scheduling
/// behavior may change in 1.x releases.
///
/// # Examples
///
/// ```
/// use tokio::runtime::LlcAwareConfig;
///
/// // Two LLC partitions on the same NUMA node and one on another node.
/// let mut config = LlcAwareConfig::new(3, |worker| worker.map(|worker| worker % 3));
/// config.numa_node_map([0, 0, 1]);
///
/// assert_eq!(config.partition_count(), 3);
/// assert_eq!(config.numa_node_count(), 2);
/// ```
///
/// [unstable]: crate#unstable-features
#[derive(Clone)]
pub struct LlcAwareConfig {
    pub(crate) partition_count: usize,
    pub(crate) partition_numa_nodes: Arc<[usize]>,
    pub(crate) numa_node_count: usize,
    pub(crate) current_partition: CurrentPartition,
    pub(crate) task_hint: Option<TaskHintCallback>,
    pub(crate) refresh_interval: u32,
    pub(crate) cross_llc_scan_limit: usize,
    pub(crate) cross_numa_scan_limit: usize,
    pub(crate) cross_llc_steal_batch: usize,
    pub(crate) cross_numa_steal_batch: usize,
}

pub(crate) type CurrentPartition = Arc<dyn Fn(Option<usize>) -> Option<usize> + Send + Sync>;
pub(crate) type TaskHintCallback =
    Arc<dyn Fn(&TaskMeta<'_>) -> LlcTaskHint + Send + Sync>;

const DEFAULT_REFRESH_INTERVAL: u32 = 61;
const DEFAULT_CROSS_LLC_SCAN_LIMIT: usize = 4;
const DEFAULT_CROSS_NUMA_SCAN_LIMIT: usize = 1;
const DEFAULT_CROSS_LLC_STEAL_BATCH: usize = 32;
const DEFAULT_CROSS_NUMA_STEAL_BATCH: usize = 1;

impl LlcAwareConfig {
    /// Creates an LLC-aware scheduler configuration.
    ///
    /// `current_partition` receives the runtime worker index, or `None` when
    /// work is submitted from outside a runtime worker, and returns the LLC
    /// partition on which the calling thread is currently executing. The
    /// callback should be inexpensive: it is invoked for external submissions,
    /// after every worker unpark, and periodically while a worker remains busy.
    /// A zero `partition_count` falls back to one partition.
    pub fn new<F>(partition_count: usize, current_partition: F) -> Self
    where
        F: Fn(Option<usize>) -> Option<usize> + Send + Sync + 'static,
    {
        let partition_count = fallback_if_zero("llc_partitions", partition_count, 1);

        Self {
            partition_count,
            partition_numa_nodes: vec![0; partition_count].into(),
            numa_node_count: 1,
            current_partition: Arc::new(current_partition),
            task_hint: None,
            refresh_interval: DEFAULT_REFRESH_INTERVAL,
            cross_llc_scan_limit: DEFAULT_CROSS_LLC_SCAN_LIMIT,
            cross_numa_scan_limit: DEFAULT_CROSS_NUMA_SCAN_LIMIT,
            cross_llc_steal_batch: DEFAULT_CROSS_LLC_STEAL_BATCH,
            cross_numa_steal_batch: DEFAULT_CROSS_NUMA_STEAL_BATCH,
        }
    }

    /// Returns the number of configured LLC partitions.
    pub fn partition_count(&self) -> usize {
        self.partition_count
    }

    /// Configures the NUMA node containing each LLC partition.
    ///
    /// The iterator must contain exactly one node identifier per LLC
    /// partition. Node identifiers do not need to be dense; Tokio normalizes
    /// them internally. Without this mapping, every LLC is treated as part of
    /// a single NUMA node.
    ///
    /// If the number of entries differs from [`partition_count`], Tokio falls
    /// back to treating all LLCs as part of one NUMA node.
    ///
    /// [`partition_count`]: Self::partition_count
    pub fn numa_node_map<I>(&mut self, nodes: I) -> &mut Self
    where
        I: IntoIterator<Item = usize>,
    {
        let nodes: Vec<_> = nodes.into_iter().collect();
        if nodes.len() != self.partition_count {
            warn_fallback("numa_node_map length", nodes.len(), 1);
            self.partition_numa_nodes = vec![0; self.partition_count].into();
            self.numa_node_count = 1;
            return self;
        }

        let mut unique = nodes.clone();
        unique.sort_unstable();
        unique.dedup();
        self.partition_numa_nodes = nodes
            .into_iter()
            .map(|node| unique.binary_search(&node).unwrap())
            .collect::<Vec<_>>()
            .into();
        self.numa_node_count = unique.len();
        self
    }

    /// Returns the number of configured NUMA nodes.
    pub fn numa_node_count(&self) -> usize {
        self.numa_node_count
    }

    /// Sets how many scheduler ticks may pass between partition checks while a
    /// worker remains busy.
    ///
    /// Workers always recheck after waking, independently of this interval.
    /// The default is 61 ticks. Zero restores that default.
    pub fn refresh_interval(&mut self, interval: u32) -> &mut Self {
        self.refresh_interval = if interval == 0 {
            warn_fallback("refresh_interval", 0, DEFAULT_REFRESH_INTERVAL as usize);
            DEFAULT_REFRESH_INTERVAL
        } else {
            interval
        };
        self
    }

    /// Sets the maximum number of remote LLC queues and remote worker queues
    /// examined in each cross-LLC search rung.
    ///
    /// Same-LLC workers are still scanned exhaustively. Keeping only the
    /// expensive cross-cache probes bounded prevents idle-worker search cost
    /// from growing linearly with a large machine's worker count. The default
    /// is 4. Zero restores that default.
    pub fn cross_llc_scan_limit(&mut self, limit: usize) -> &mut Self {
        self.cross_llc_scan_limit = fallback_if_zero(
            "cross_llc_scan_limit",
            limit,
            DEFAULT_CROSS_LLC_SCAN_LIMIT,
        );
        self
    }

    /// Sets the maximum number of worker and LLC queues examined when a worker
    /// searches across a NUMA boundary.
    ///
    /// The default is 1 because remote-memory movement is more expensive than
    /// an LLC-only migration. Zero restores that default.
    pub fn cross_numa_scan_limit(&mut self, limit: usize) -> &mut Self {
        self.cross_numa_scan_limit = fallback_if_zero(
            "cross_numa_scan_limit",
            limit,
            DEFAULT_CROSS_NUMA_SCAN_LIMIT,
        );
        self
    }

    /// Sets the maximum number of tasks moved from a worker in another LLC on
    /// the same NUMA node. Tokio still takes no more than half of the victim's
    /// queue. The default is 32. Zero restores that default.
    pub fn cross_llc_steal_batch(&mut self, tasks: usize) -> &mut Self {
        self.cross_llc_steal_batch = fallback_if_zero(
            "cross_llc_steal_batch",
            tasks,
            DEFAULT_CROSS_LLC_STEAL_BATCH,
        );
        self
    }

    /// Sets the maximum number of tasks moved from a worker on another NUMA
    /// node. Tokio still takes no more than half of the victim's queue. The
    /// default is 1 to avoid migrating a large working set across nodes. Zero
    /// restores that default.
    pub fn cross_numa_steal_batch(&mut self, tasks: usize) -> &mut Self {
        self.cross_numa_steal_batch = fallback_if_zero(
            "cross_numa_steal_batch",
            tasks,
            DEFAULT_CROSS_NUMA_STEAL_BATCH,
        );
        self
    }

    /// Sets a callback that supplies a placement hint when a task is enqueued
    /// into a shared scheduler queue.
    ///
    /// By default, a task inherits the partition on which it was last polled.
    /// The callback may override that placement or route the task through the
    /// global queue. The callback is only used when LLC-aware scheduling is
    /// enabled. The callback runs before options set directly on
    /// [`task::Builder`](crate::task::Builder) are applied, so those options
    /// take precedence over its result.
    pub fn on_task_enqueue<F>(&mut self, f: F) -> &mut Self
    where
        F: Fn(&TaskMeta<'_>) -> LlcTaskHint + Send + Sync + 'static,
    {
        self.task_hint = Some(Arc::new(f));
        self
    }

    pub(crate) fn current_partition(&self, worker: Option<usize>) -> Option<usize> {
        (self.current_partition)(worker).filter(|partition| *partition < self.partition_count)
    }

    pub(crate) fn numa_node(&self, partition: usize) -> usize {
        self.partition_numa_nodes[partition]
    }

    /// Creates a configuration by discovering the host's LLC topology on
    /// Linux.
    ///
    /// The process's allowed CPU set and sysfs topology are read once. The
    /// generated callback uses `sched_getcpu(3)` to determine the worker's
    /// current CPU, then performs a table lookup to find its dense LLC
    /// partition identifier.
    #[cfg(target_os = "linux")]
    #[cfg_attr(docsrs, doc(cfg(target_os = "linux")))]
    pub fn from_linux_topology() -> std::io::Result<Self> {
        linux::discover()
    }
}

fn fallback_if_zero(name: &'static str, value: usize, fallback: usize) -> usize {
    if value == 0 {
        warn_fallback(name, value, fallback);
        fallback
    } else {
        value
    }
}

fn warn_fallback(name: &'static str, supplied: usize, fallback: usize) {
    #[cfg(feature = "tracing")]
    tracing::warn!(
        target: "tokio::runtime",
        setting = name,
        supplied,
        fallback,
        "invalid LLC-aware scheduler setting; using fallback"
    );

    #[cfg(not(feature = "tracing"))]
    let _ = (name, supplied, fallback);
}

impl fmt::Debug for LlcAwareConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LlcAwareConfig")
            .field("partition_count", &self.partition_count)
            .field("numa_node_count", &self.numa_node_count)
            .field("current_partition", &"...")
            .field("task_hint", &self.task_hint.as_ref().map(|_| "..."))
            .field("refresh_interval", &self.refresh_interval)
            .field("cross_llc_scan_limit", &self.cross_llc_scan_limit)
            .field("cross_numa_scan_limit", &self.cross_numa_scan_limit)
            .field("cross_llc_steal_batch", &self.cross_llc_steal_batch)
            .field("cross_numa_steal_batch", &self.cross_numa_steal_batch)
            .finish()
    }
}

/// A userspace placement hint for a task entering an LLC queue.
///
/// A task inherits the LLC on which it was last polled unless the placement is
/// overridden.
///
/// # Examples
///
/// ```
/// use tokio::runtime::LlcTaskHint;
///
/// let hint = LlcTaskHint::new().with_partition(1);
/// # let _ = hint;
/// ```
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LlcTaskHint {
    pub(crate) placement: LlcTaskPlacement,
}

impl LlcTaskHint {
    /// Creates a hint that inherits the task's previous LLC.
    pub const fn new() -> Self {
        Self {
            placement: LlcTaskPlacement::Inherit,
        }
    }

    /// Routes the task to `partition`.
    pub const fn with_partition(mut self, partition: usize) -> Self {
        self.placement = LlcTaskPlacement::Partition(partition);
        self
    }

    /// Routes the task through the global queue.
    pub const fn with_global_queue(mut self) -> Self {
        self.placement = LlcTaskPlacement::Global;
        self
    }
}

impl Default for LlcTaskHint {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LlcTaskPlacement {
    Inherit,
    Global,
    Partition(usize),
}

/// Per-task overrides carried from `task::Builder` through `SpawnMeta`.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct LlcTaskOptions {
    pub(crate) placement: Option<LlcTaskPlacement>,
}

#[cfg(target_os = "linux")]
mod linux {
    use super::LlcAwareConfig;

    use std::collections::BTreeMap;
    use std::fs;
    use std::io;
    use std::sync::Arc;

    const NO_PARTITION: usize = usize::MAX;

    pub(super) fn discover() -> io::Result<LlcAwareConfig> {
        let cpus = allowed_cpus()?;
        let max_cpu = cpus.iter().copied().max().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "CPU online list is empty")
        })?;

        let mut llcs = BTreeMap::<String, usize>::new();
        let mut cpu_partitions = vec![NO_PARTITION; max_cpu + 1];
        let mut partition_nodes = Vec::new();

        for cpu in cpus {
            let key = llc_key(cpu)?;
            let partition = if let Some(partition) = llcs.get(&key) {
                *partition
            } else {
                let partition = llcs.len();
                llcs.insert(key, partition);
                partition_nodes.push(numa_node(cpu).unwrap_or(0));
                partition
            };
            cpu_partitions[cpu] = partition;
        }

        if llcs.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "no last-level cache topology was found",
            ));
        }

        let cpu_partitions: Arc<[usize]> = cpu_partitions.into();
        let mut config = LlcAwareConfig::new(llcs.len(), move |_| {
            let cpu = current_cpu()?;
            cpu_partitions
                .get(cpu)
                .copied()
                .filter(|partition| *partition != NO_PARTITION)
        });
        config.numa_node_map(partition_nodes);
        Ok(config)
    }

    fn allowed_cpus() -> io::Result<Vec<usize>> {
        if let Ok(status) = fs::read_to_string("/proc/self/status") {
            if let Some(value) = status
                .lines()
                .find_map(|line| line.strip_prefix("Cpus_allowed_list:"))
            {
                if let Ok(cpus) = parse_cpu_list(value.trim()) {
                    if !cpus.is_empty() {
                        return Ok(cpus);
                    }
                }
            }
        }

        parse_cpu_list(&fs::read_to_string("/sys/devices/system/cpu/online")?)
    }

    fn llc_key(cpu: usize) -> io::Result<String> {
        let cache_dir = format!("/sys/devices/system/cpu/cpu{cpu}/cache");
        let mut best = None::<(u32, String)>;

        for entry in fs::read_dir(cache_dir)? {
            let path = entry?.path();
            let Ok(level) = read_trimmed(path.join("level")).and_then(|level| {
                level
                    .parse::<u32>()
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
            }) else {
                continue;
            };
            let Ok(kind) = read_trimmed(path.join("type")) else {
                continue;
            };
            if kind != "Unified" && kind != "Data" {
                continue;
            }

            let Ok(identity) = read_trimmed(path.join("id"))
                .or_else(|_| read_trimmed(path.join("shared_cpu_list")))
            else {
                continue;
            };
            if best
                .as_ref()
                .map_or(true, |(best_level, _)| level > *best_level)
            {
                best = Some((level, format!("{level}:{identity}")));
            }
        }

        if let Some((_, key)) = best {
            return Ok(key);
        }

        read_trimmed(format!(
            "/sys/devices/system/cpu/cpu{cpu}/topology/llc_id"
        ))
    }

    fn read_trimmed(path: impl AsRef<std::path::Path>) -> io::Result<String> {
        Ok(fs::read_to_string(path)?.trim().to_owned())
    }

    fn numa_node(cpu: usize) -> io::Result<usize> {
        let cpu_dir = format!("/sys/devices/system/cpu/cpu{cpu}");
        for entry in fs::read_dir(cpu_dir)? {
            let name = entry?.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if let Some(node) = name.strip_prefix("node") {
                if !node.is_empty() && node.bytes().all(|byte| byte.is_ascii_digit()) {
                    return parse_cpu(node);
                }
            }
        }
        Ok(0)
    }

    fn parse_cpu_list(list: &str) -> io::Result<Vec<usize>> {
        let mut cpus = Vec::new();
        for range in list.trim().split(',') {
            let (start, end) = match range.split_once('-') {
                Some((start, end)) => (parse_cpu(start)?, parse_cpu(end)?),
                None => {
                    let cpu = parse_cpu(range)?;
                    (cpu, cpu)
                }
            };
            if start > end {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid CPU range",
                ));
            }
            cpus.extend(start..=end);
        }
        Ok(cpus)
    }

    fn parse_cpu(value: &str) -> io::Result<usize> {
        value
            .parse()
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    fn current_cpu() -> Option<usize> {
        extern "C" {
            fn sched_getcpu() -> std::os::raw::c_int;
        }

        // SAFETY: sched_getcpu takes no arguments and has no memory-safety
        // preconditions. A negative return value indicates an OS error.
        let cpu = unsafe { sched_getcpu() };
        (cpu >= 0).then_some(cpu as usize)
    }

    #[test]
    fn parses_cpu_lists() {
        assert_eq!(parse_cpu_list("0-2,5,7-8\n").unwrap(), [0, 1, 2, 5, 7, 8]);
        assert!(parse_cpu_list("4-2").is_err());
    }
}
