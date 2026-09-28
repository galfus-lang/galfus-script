use super::*;

use crate::state::ModuleLifecycle;
use galfus_core::ModulePath;

#[test]
fn check_and_compile_advance_the_loaded_module_state_record() {
    let mut workspace = Workspace::new();
    workspace
        .load_manifest(
            toml::from_str(
                r#"
                [module]
                name = "module-state"
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
            b"export fn main(args: [[u8]]): i32 { return 0 }",
        )
        .expect("source loads");

    let module_path = ModulePath::new("main.gfs").expect("valid module path");
    let module_id = workspace
        .source_state
        .store
        .get(&module_path)
        .expect("source entry exists")
        .module_id;
    assert_eq!(
        workspace
            .semantic_state
            .module_states
            .get(module_id)
            .expect("state is created with the source")
            .lifecycle(),
        ModuleLifecycle::SourceLoaded
    );

    assert!(workspace.check().is_valid);
    assert_eq!(
        workspace
            .semantic_state
            .module_states
            .get(module_id)
            .expect("state remains keyed by ModuleId")
            .lifecycle(),
        ModuleLifecycle::InterfaceValid
    );

    workspace.compile().expect("workspace compiles");
    let record = workspace
        .semantic_state
        .module_states
        .get(module_id)
        .expect("same state is consumed by compilation");
    assert_eq!(record.lifecycle(), ModuleLifecycle::BytecodeProduced);
    assert!(record.source_hash().is_some());
    assert!(record.interface_hash().is_some());
    assert!(record.bytecode_hash().is_some());
}
