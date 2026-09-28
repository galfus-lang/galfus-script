use galfus_core::ModuleId;
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SourceProducerWorkCounters {
    pub(crate) semantic_graph_modules_inspected: usize,
    pub(crate) last_dependency_closure_size: usize,
    pub(crate) cached_node_hits: usize,
    pub(crate) compiled_node_count: usize,
}

pub(super) struct SourceProducerCounterSet {
    semantic_graph_modules_inspected: AtomicUsize,
    last_dependency_closure_size: AtomicUsize,
    cached_node_hits: AtomicUsize,
    compiled_node_count: AtomicUsize,
    production_counts: Mutex<HashMap<ModuleId, usize>>,
}

impl Default for SourceProducerCounterSet {
    fn default() -> Self {
        Self {
            semantic_graph_modules_inspected: AtomicUsize::new(0),
            last_dependency_closure_size: AtomicUsize::new(0),
            cached_node_hits: AtomicUsize::new(0),
            compiled_node_count: AtomicUsize::new(0),
            production_counts: Mutex::new(HashMap::new()),
        }
    }
}

impl SourceProducerCounterSet {
    pub(super) fn snapshot(&self) -> SourceProducerWorkCounters {
        SourceProducerWorkCounters {
            semantic_graph_modules_inspected: self
                .semantic_graph_modules_inspected
                .load(Ordering::Relaxed),
            last_dependency_closure_size: self.last_dependency_closure_size.load(Ordering::Relaxed),
            cached_node_hits: self.cached_node_hits.load(Ordering::Relaxed),
            compiled_node_count: self.compiled_node_count.load(Ordering::Relaxed),
        }
    }

    pub(super) fn production_count(&self, module_id: ModuleId) -> usize {
        self.production_counts
            .lock()
            .expect("workspace source producer test counters are available")
            .get(&module_id)
            .copied()
            .unwrap_or_default()
    }

    pub(super) fn record_cached_node_hit(&self) {
        self.cached_node_hits.fetch_add(1, Ordering::Relaxed);
    }

    pub(super) fn record_production(&self, module_id: ModuleId) {
        let mut counts = self
            .production_counts
            .lock()
            .expect("workspace source producer test counters are available");
        *counts.entry(module_id).or_default() += 1;
    }

    pub(super) fn record_semantic_graph_inspection(&self, count: usize) {
        self.semantic_graph_modules_inspected
            .fetch_add(count, Ordering::Relaxed);
    }

    pub(super) fn record_dependency_closure(&self, count: usize) {
        self.last_dependency_closure_size
            .store(count, Ordering::Relaxed);
    }

    pub(super) fn record_compiled_nodes(&self, count: usize) {
        self.compiled_node_count.fetch_add(count, Ordering::Relaxed);
    }
}
