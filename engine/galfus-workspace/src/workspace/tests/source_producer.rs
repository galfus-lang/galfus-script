use super::*;
use crate::state::WorkspaceModuleCatalog;
use galfus_core::ModuleId;
use galfus_runtime::ModuleResolver;
use std::sync::{Arc, Barrier};
use std::thread;

#[test]
fn source_producer_shares_concurrent_module_requests_without_compiling_disconnected_sources() {
    let workspace = checked_workspace();
    let catalog = workspace.module_catalog().expect("checked module catalog");
    let main_id = module_id(catalog, "main.gfs");
    let producer = workspace
        .source_module_producer()
        .expect("checked workspace creates a source producer");
    let resolver = Arc::new(ModuleResolver::new(
        &catalog.runtime_catalog().expect("runtime catalog"),
        producer.clone(),
    ));
    let barrier = Arc::new(Barrier::new(4));
    let handles = (0..4)
        .map(|_| {
            let barrier = Arc::clone(&barrier);
            let resolver = Arc::clone(&resolver);
            thread::spawn(move || {
                barrier.wait();
                resolver
                    .ensure_module(main_id)
                    .expect("main module resolves")
                    .id()
            })
        })
        .collect::<Vec<_>>();

    for handle in handles {
        assert_eq!(handle.join().expect("module request thread"), main_id);
    }
    assert_eq!(producer.production_count(main_id), 1);
    let counters = producer.work_counters();
    assert_eq!(counters.semantic_graph_modules_inspected, 2);
    assert_eq!(counters.last_dependency_closure_size, 2);
    assert_eq!(counters.compiled_node_count, 2);
}

fn checked_workspace() -> Workspace {
    let mut workspace = Workspace::new();
    workspace
        .load_manifest(
            toml::from_str(
                r#"
                [module]
                name = "source-producer-concurrent"
                target = "app"
                [entry]
                path = "main.gfs"
                "#,
            )
            .expect("valid configuration"),
        )
        .expect("configuration loads");
    workspace
        .load_module(
            "main.gfs",
            b"import { value } from \"./dependency\"\nexport fn main(args: [[u8]]): i32 { return value() }",
        )
        .expect("main source loads");
    workspace
        .load_module("dependency.gfs", b"export fn value(): i32 { return 7 }")
        .expect("dependency source loads");
    workspace
        .load_module("detached.gfs", b"export fn detached(): i32 { return 9 }")
        .expect("detached source loads");
    assert!(workspace.check().is_valid);
    workspace
}

fn module_id(catalog: &WorkspaceModuleCatalog, path: &str) -> ModuleId {
    catalog
        .iter()
        .find(|descriptor| descriptor.module_path().as_str() == path)
        .expect("module descriptor")
        .module_id()
}
