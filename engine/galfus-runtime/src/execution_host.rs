use std::rc::Rc;
use std::sync::Arc;

use crate::driver::ExecutionDriver;
use crate::{AdapterBindingPreflight, Execution, PreflightError, Runtime, RuntimeError};
use galfus_bytecode::PackageImage;
use galfus_contract::{AdapterLoadContext, AdapterModuleLoader, Providers, RuntimeCapabilities};

/// The host-side bootstrap boundary for one package execution.
///
/// It resolves the package's adapter declarations, seals the resulting bindings into runtime
/// capabilities, and starts the execution only after all preflight checks succeed.
pub struct ExecutionHost {
    context: AdapterLoadContext,
    providers: Option<Providers>,
    adapter_preflight: AdapterBindingPreflight,
}

#[derive(Debug, thiserror::Error)]
pub enum HostBootstrapError {
    #[error("package preflight failed: {0}")]
    Preflight(#[from] PreflightError),
    #[error("runtime bootstrap failed: {0}")]
    Runtime(#[from] RuntimeError),
}

impl ExecutionHost {
    pub fn new(context: AdapterLoadContext) -> Self {
        Self {
            context,
            providers: None,
            adapter_preflight: AdapterBindingPreflight::new(),
        }
    }

    pub fn with_providers(mut self, providers: Providers) -> Self {
        self.providers = Some(providers);
        self
    }

    pub fn register_adapter_loader(
        &mut self,
        adapter_name: impl Into<String>,
        loader: Box<dyn AdapterModuleLoader>,
    ) -> Result<(), PreflightError> {
        self.adapter_preflight.register_loader(adapter_name, loader)
    }

    /// Resolves and validates all externally supplied capabilities before creating the runtime.
    pub fn start(
        self,
        package: Arc<PackageImage>,
        args: &[Vec<u8>],
        driver: Rc<dyn ExecutionDriver>,
    ) -> Result<Execution, HostBootstrapError> {
        let bindings = self
            .adapter_preflight
            .bind_package(&package, &self.context)?;
        let capabilities = self
            .providers
            .map_or_else(RuntimeCapabilities::builder, |providers| {
                RuntimeCapabilities::builder().with_providers(providers)
            });

        Runtime::new(
            package,
            capabilities.with_adapter_bindings(bindings).build(),
        )
        .start(args, driver)
        .map_err(HostBootstrapError::Runtime)
    }
}
