pub mod driver;
pub mod providers;
use galfus_bytecode::PackageImage;
use galfus_contract::{AdapterBindings, ExecutionFailure, Providers, RuntimeCapabilities};
use galfus_runtime::Runtime;
use galfus_runtime::driver::ExecutionDriver;
use galfus_contract::CapabilityCatalog;
use std::rc::Rc;
use std::sync::Arc;

pub fn native_catalog() -> CapabilityCatalog {
    galfus_contract::CapabilityCatalog::new(
        vec![
            galfus_contract::BridgeModule::new("std/io", galfus_contract::builtins::STD_IO_SOURCE),
            galfus_contract::BridgeModule::new("std/env", galfus_contract::builtins::STD_ENV_SOURCE),
            galfus_contract::BridgeModule::new("std/time", galfus_contract::builtins::STD_TIME_SOURCE),
            galfus_contract::BridgeModule::new("std/fs", galfus_contract::builtins::STD_FS_SOURCE),
        ],
        Vec::new(),
    )
    .expect("the native provider catalog is valid")
}

pub struct PackageLoader {
    // Defines paths and mechanisms to load dynamic libraries for adapters
}

impl PackageLoader {
    pub fn new() -> Self {
        Self {}
    }

    pub fn load_from_bytes(&self, bytes: &[u8]) -> Result<PackageImage, String> {
        PackageImage::from_bytecode(bytes).map_err(|e| e.to_string())
    }
}

pub struct ExecutionHost {
    providers: Providers,
    adapters: AdapterBindings,
    driver: Rc<dyn ExecutionDriver>,
}

impl ExecutionHost {
    pub fn new(
        providers: Providers,
        adapters: AdapterBindings,
        driver: Rc<dyn ExecutionDriver>,
    ) -> Self {
        Self {
            providers,
            adapters,
            driver,
        }
    }

    pub fn run(
        self,
        package: Arc<PackageImage>,
        args: &[Vec<u8>],
    ) -> Result<i32, ExecutionFailure> {
        let capabilities = RuntimeCapabilities::builder()
            .with_providers(self.providers)
            .with_adapter_bindings(self.adapters)
            .build();

        let runtime = Runtime::new(package, capabilities);

        let mut execution = runtime.start(args, self.driver.clone()).map_err(|e| {
            ExecutionFailure::new(
                galfus_contract::ExecutionFailureKind::InitializationFailure,
                e.to_string(),
            )
        })?;

        let result = execution.run_sync_to_completion().map_err(|e| {
            ExecutionFailure::new(
                galfus_contract::ExecutionFailureKind::InternalRuntimeFailure,
                e.to_string(),
            )
        })?;

        if let galfus_contract::BoundaryValue::I32(code) = result {
            Ok(code)
        } else {
            Ok(0)
        }
    }
}
