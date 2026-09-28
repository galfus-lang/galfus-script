use super::*;

#[test]
fn runtime_start_overhead_excludes_the_first_source_node_production() {
    let collector = WorkspaceTimingCollector::default();

    collector.record_first_source_node_production(Duration::from_micros(30));
    collector.record_runtime_start(Duration::from_micros(80));

    let timing = collector.snapshot();
    assert_eq!(timing.first_source_node_production_us, Some(30));
    assert_eq!(timing.runtime_start_overhead_us, Some(50));
}
