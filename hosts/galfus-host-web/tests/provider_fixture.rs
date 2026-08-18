use std::path::PathBuf;
use std::sync::Arc;

use galfus_contract::{BridgeModule, CapabilityCatalog};
use galfus_workspace::Workspace;

const PROVIDER_FIXTURE: &str = include_str!("fixtures/providers.gfs");

fn provider_catalog() -> CapabilityCatalog {
    let providers = galfus_contract::builtins::BRIDGE_TEMPLATES
        .iter()
        .map(|(name, source)| BridgeModule::new(*name, *source))
        .collect();
    CapabilityCatalog::new(providers, Vec::new()).expect("provider catalog is valid")
}

#[test]
fn compiles_web_provider_fixture() {
    let mut workspace = Workspace::new();
    workspace.set_catalog(Arc::new(provider_catalog()));
    workspace
        .load_manifest(
            toml::from_str(
                r#"
            [module]
            name = "host-web-provider-suite"
            target = "app"

            [entry]
            path = "main.gfs"
            "#,
            )
            .unwrap(),
        )
        .expect("fixture configuration is valid");
    workspace
        .load_module("main.gfs", PROVIDER_FIXTURE.as_bytes())
        .expect("fixture source loads");

    let report = workspace.check();
    assert!(
        report.is_valid,
        "fixture typecheck failed: {:#?}",
        report.diagnostics
    );

    let package = workspace.compile().expect("fixture compiles").package;
    let output_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("target/host-web-provider-suite");
    std::fs::create_dir_all(&output_dir).expect("fixture output directory exists");
    std::fs::write(
        output_dir.join("providers.bin"),
        package.to_bytecode().expect("fixture serializes"),
    )
    .expect("fixture bytecode writes");
}
