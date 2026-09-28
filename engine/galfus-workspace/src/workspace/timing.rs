#[cfg(test)]
mod tests;

use serde::Serialize;
use std::sync::Mutex;
use std::time::Duration;

/// Timing data emitted only when a workspace caller installs a collector.
#[derive(Debug, Clone, Default, Serialize)]
pub struct WorkspaceRuntimeTiming {
    pub first_source_node_production_us: Option<u64>,
    pub runtime_start_overhead_us: Option<u64>,
}

/// Collects timing work that occurs below the CLI boundary.
#[derive(Debug, Default)]
pub struct WorkspaceTimingCollector {
    timing: Mutex<WorkspaceRuntimeTiming>,
}

impl WorkspaceTimingCollector {
    pub fn snapshot(&self) -> WorkspaceRuntimeTiming {
        self.timing
            .lock()
            .expect("workspace timing collector is available")
            .clone()
    }

    pub(crate) fn record_first_source_node_production(&self, elapsed: Duration) {
        let mut timing = self
            .timing
            .lock()
            .expect("workspace timing collector is available");
        if timing.first_source_node_production_us.is_none() {
            timing.first_source_node_production_us = Some(elapsed.as_micros() as u64);
        }
    }

    pub(crate) fn record_runtime_start(&self, elapsed: Duration) {
        let mut timing = self
            .timing
            .lock()
            .expect("workspace timing collector is available");
        let production = timing.first_source_node_production_us.unwrap_or_default();
        timing.runtime_start_overhead_us =
            Some((elapsed.as_micros() as u64).saturating_sub(production));
    }
}
