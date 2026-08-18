use std::rc::Rc;
use std::sync::Arc;

use galfus_contract::AdapterBindings;
use galfus_host_native::{ExecutionHost, driver::NativeDriver, native_catalog, providers};
use galfus_workspace::Workspace;

const PROVIDER_FIXTURE: &str = include_str!("fixtures/providers.gfs");

fn compile_fixture(source: &str) -> Arc<galfus_bytecode::PackageImage> {
    let mut workspace = Workspace::new();
    workspace.set_catalog(Arc::new(native_catalog()));
    workspace
        .load_manifest(
            toml::from_str(
                r#"
            [module]
            name = "host-native-provider-suite"
            target = "app"

            [entry]
            path = "main.gfs"
            "#,
            )
            .unwrap(),
        )
        .expect("fixture configuration is valid");
    workspace
        .load_module("main.gfs", source.as_bytes())
        .expect("fixture source loads");

    let report = workspace.check();
    assert!(
        report.is_valid,
        "fixture typecheck failed: {:#?}",
        report.diagnostics
    );

    workspace.compile().expect("fixture compiles").package
}

#[test]
fn provider_fixture_executes_against_native_providers() {
    let package = compile_fixture(PROVIDER_FIXTURE);
    let providers = providers::default_providers(package.metadata().clone());
    let host = ExecutionHost::new(
        providers,
        AdapterBindings::default(),
        Rc::new(NativeDriver::new()),
    );

    let exit_code = host.run(package, &[]).expect("fixture execution succeeds");
    assert_eq!(exit_code, 0);
}
