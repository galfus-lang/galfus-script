use super::*;
use crate::{AdapterValidationError, ExternalModuleDescriptor};

struct TestAdapter(&'static str);

impl ExternalAdapterSchema for TestAdapter {
    fn name(&self) -> &str {
        "test"
    }

    fn catalog_schema(&self) -> String {
        self.0.to_string()
    }

    fn validate_schema(
        &self,
        _descriptor: &ExternalModuleDescriptor,
    ) -> Result<(), AdapterValidationError> {
        Ok(())
    }
}

#[test]
fn provider_source_changes_the_fingerprint() {
    let first = CapabilityCatalog::new(
        vec![BridgeModule::new(
            "std/example",
            "export fn one(): i32 { return 1 }",
        )],
        Vec::new(),
    )
    .expect("valid provider catalog");
    let second = CapabilityCatalog::new(
        vec![BridgeModule::new(
            "std/example",
            "export fn one(): i32 { return 2 }",
        )],
        Vec::new(),
    )
    .expect("valid provider catalog");

    assert_ne!(first.fingerprint(), second.fingerprint());
}

#[test]
fn adapter_schema_changes_the_fingerprint() {
    let first = CapabilityCatalog::new(
        Vec::new(),
        vec![Arc::new(TestAdapter("fn call(value: i32): i32"))],
    )
    .expect("valid adapter catalog");
    let second = CapabilityCatalog::new(
        Vec::new(),
        vec![Arc::new(TestAdapter("fn call(value: i64): i64"))],
    )
    .expect("valid adapter catalog");

    assert_ne!(first.fingerprint(), second.fingerprint());
}

#[test]
fn catalog_rejects_duplicate_and_builtin_provider_paths() {
    let duplicate = CapabilityCatalog::new(
        vec![
            BridgeModule::new("std/example", ""),
            BridgeModule::new("std/example", ""),
        ],
        Vec::new(),
    );
    assert!(matches!(
        duplicate,
        Err(CapabilityCatalogError::DuplicateProviderPath(path)) if path == "std/example"
    ));

    let builtin = CapabilityCatalog::new(vec![BridgeModule::new("std/async", "")], Vec::new());
    assert!(matches!(
        builtin,
        Err(CapabilityCatalogError::ProviderBuiltinConflict(path)) if path == "std/async"
    ));
}
