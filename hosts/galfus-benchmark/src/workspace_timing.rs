use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_TIMING_FILE: AtomicUsize = AtomicUsize::new(0);

pub(super) const TIMING_FILE_ENV: &str = "GALFUS_WORKSPACE_TIMING_FILE";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(super) struct WorkspaceTiming {
    pub(super) process_bootstrap_us: u64,
    pub(super) manifest_catalog_setup_us: u64,
    pub(super) source_discovery_us: u64,
    pub(super) source_reads_us: u64,
    pub(super) interface_validation_us: u64,
    pub(super) first_source_node_production_us: Option<u64>,
    pub(super) runtime_start_overhead_us: Option<u64>,
    pub(super) entry_completion_us: Option<u64>,
}

impl WorkspaceTiming {
    pub(super) fn output_path(command: &[String]) -> Option<PathBuf> {
        let is_workspace_run = command
            .windows(2)
            .any(|arguments| arguments[0].contains("galfus-cli") && arguments[1] == "run");
        if !is_workspace_run {
            return None;
        }
        let directory = Path::new(".tmp/benchmark/workspace-timing");
        std::fs::create_dir_all(directory).ok()?;
        Some(directory.join(format!(
            "{}-{}-{}.json",
            std::process::id(),
            NEXT_TIMING_FILE.fetch_add(1, Ordering::Relaxed),
            command.len()
        )))
    }

    pub(super) fn read(path: &Path) -> Result<Self, String> {
        let contents = std::fs::read(path).map_err(|error| error.to_string())?;
        serde_json::from_slice(contents.as_slice()).map_err(|error| error.to_string())
    }

    pub(super) fn median(samples: impl Iterator<Item = Self>) -> Option<Self> {
        let samples = samples.collect::<Vec<_>>();
        (!samples.is_empty()).then(|| Self {
            process_bootstrap_us: median_required(&samples, |timing| timing.process_bootstrap_us),
            manifest_catalog_setup_us: median_required(&samples, |timing| {
                timing.manifest_catalog_setup_us
            }),
            source_discovery_us: median_required(&samples, |timing| timing.source_discovery_us),
            source_reads_us: median_required(&samples, |timing| timing.source_reads_us),
            interface_validation_us: median_required(&samples, |timing| {
                timing.interface_validation_us
            }),
            first_source_node_production_us: median_optional(&samples, |timing| {
                timing.first_source_node_production_us
            }),
            runtime_start_overhead_us: median_optional(&samples, |timing| {
                timing.runtime_start_overhead_us
            }),
            entry_completion_us: median_optional(&samples, |timing| timing.entry_completion_us),
        })
    }
}

fn median_required(samples: &[WorkspaceTiming], metric: impl Fn(&WorkspaceTiming) -> u64) -> u64 {
    let mut values = samples.iter().map(metric).collect::<Vec<_>>();
    values.sort_unstable();
    values[values.len() / 2]
}

fn median_optional(
    samples: &[WorkspaceTiming],
    metric: impl Fn(&WorkspaceTiming) -> Option<u64>,
) -> Option<u64> {
    let mut values = samples.iter().filter_map(metric).collect::<Vec<_>>();
    values.sort_unstable();
    values.get(values.len() / 2).copied()
}
