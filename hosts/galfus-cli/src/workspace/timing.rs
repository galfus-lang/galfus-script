use galfus_workspace::{WorkspaceRuntimeTiming, WorkspaceTimingCollector};
use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

pub(super) const TIMING_FILE_ENV: &str = "GALFUS_WORKSPACE_TIMING_FILE";

#[derive(Debug, Serialize)]
struct WorkspaceTimingReport {
    process_bootstrap_us: u64,
    manifest_catalog_setup_us: u64,
    source_discovery_us: u64,
    source_reads_us: u64,
    interface_validation_us: u64,
    first_source_node_production_us: Option<u64>,
    runtime_start_overhead_us: Option<u64>,
    entry_completion_us: Option<u64>,
}

pub(super) struct WorkspaceTimer {
    output_path: Option<PathBuf>,
    process_bootstrap: Duration,
    collector: Arc<WorkspaceTimingCollector>,
    manifest_catalog_setup: Duration,
    source_discovery: Duration,
    source_reads: Duration,
    interface_validation: Duration,
    entry_completion: Option<Duration>,
}

impl WorkspaceTimer {
    pub(super) fn new(process_bootstrap: Duration) -> Self {
        Self {
            output_path: std::env::var_os(TIMING_FILE_ENV).map(PathBuf::from),
            process_bootstrap,
            collector: Arc::new(WorkspaceTimingCollector::default()),
            manifest_catalog_setup: Duration::ZERO,
            source_discovery: Duration::ZERO,
            source_reads: Duration::ZERO,
            interface_validation: Duration::ZERO,
            entry_completion: None,
        }
    }

    pub(super) fn collector(&self) -> Arc<WorkspaceTimingCollector> {
        Arc::clone(&self.collector)
    }

    pub(super) fn add_manifest_catalog_setup(&mut self, elapsed: Duration) {
        self.manifest_catalog_setup += elapsed;
    }

    pub(super) fn add_source_discovery(&mut self, elapsed: Duration) {
        self.source_discovery += elapsed;
    }

    pub(super) fn add_source_reads(&mut self, elapsed: Duration) {
        self.source_reads += elapsed;
    }

    pub(super) fn add_interface_validation(&mut self, elapsed: Duration) {
        self.interface_validation += elapsed;
    }

    pub(super) fn set_entry_completion(&mut self, elapsed: Duration) {
        self.entry_completion = Some(elapsed);
    }

    pub(super) fn write(&self) {
        let Some(output_path) = &self.output_path else {
            return;
        };
        let WorkspaceRuntimeTiming {
            first_source_node_production_us,
            runtime_start_overhead_us,
        } = self.collector.snapshot();
        let report = WorkspaceTimingReport {
            process_bootstrap_us: micros(self.process_bootstrap),
            manifest_catalog_setup_us: micros(self.manifest_catalog_setup),
            source_discovery_us: micros(self.source_discovery),
            source_reads_us: micros(self.source_reads),
            interface_validation_us: micros(self.interface_validation),
            first_source_node_production_us,
            runtime_start_overhead_us,
            entry_completion_us: self.entry_completion.map(micros),
        };
        let Ok(bytes) = serde_json::to_vec(&report) else {
            return;
        };
        let temporary_path = output_path.with_extension(format!("{}.tmp", std::process::id()));
        if fs::write(temporary_path.as_path(), bytes).is_ok() {
            let _ = fs::rename(temporary_path, output_path);
        }
    }
}

fn micros(elapsed: Duration) -> u64 {
    elapsed.as_micros() as u64
}
