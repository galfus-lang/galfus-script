use super::*;

use crate::state::*;
use galfus_bytecode::PackageImage;
use galfus_runtime::{Execution, Runtime};
use std::sync::Arc;

impl Workspace {
    pub fn start_execution(
        &mut self,
        args: &[Vec<u8>],
        providers: Option<Providers>,
        driver: std::rc::Rc<dyn galfus_runtime::driver::ExecutionDriver>,
    ) -> Result<Execution, crate::state::WorkspaceRunError> {
        let package = match &self.bytecode_state.compile_state {
            CompileState::Ready { package, .. } => Arc::clone(package),
            _ => {
                return Err(crate::state::WorkspaceRunError::Blocked(
                    RunBlocked::CompileRequired,
                ));
            }
        };
        Runtime::new(
            Arc::clone(&package),
            providers
                .map_or_else(RuntimeCapabilities::builder, |providers| {
                    RuntimeCapabilities::builder().with_providers(providers)
                })
                .build(),
        )
        .start(args, driver.clone())
        .map_err(crate::state::WorkspaceRunError::RuntimeStart)
    }

    pub fn start_execution_with_bindings(
        &mut self,
        args: &[Vec<u8>],
        providers: Option<Providers>,
        bindings: galfus_contract::AdapterBindings,
        driver: std::rc::Rc<dyn galfus_runtime::driver::ExecutionDriver>,
    ) -> Result<Execution, crate::state::WorkspaceRunError> {
        let package = match &self.bytecode_state.compile_state {
            CompileState::Ready { package, .. } => Arc::clone(package),
            _ => {
                return Err(crate::state::WorkspaceRunError::Blocked(
                    RunBlocked::CompileRequired,
                ));
            }
        };
        Runtime::new(
            Arc::clone(&package),
            providers
                .map_or_else(RuntimeCapabilities::builder, |providers| {
                    RuntimeCapabilities::builder().with_providers(providers)
                })
                .with_adapter_bindings(bindings)
                .build(),
        )
        .start(args, driver.clone())
        .map_err(crate::state::WorkspaceRunError::RuntimeStart)
    }

    /// Compatibility helper that drives the returned execution through the supplied driver.
    pub fn run(
        &mut self,
        args: &[Vec<u8>],
        providers: Option<Providers>,
        driver: std::rc::Rc<dyn galfus_runtime::driver::ExecutionDriver>,
    ) -> Result<galfus_contract::BoundaryValue, crate::state::WorkspaceRunError> {
        let mut execution = self.start_execution(args, providers, driver)?;
        execution
            .run_sync_to_completion()
            .map_err(crate::state::WorkspaceRunError::ExecutionFailed)
    }

    pub fn run_with_bindings(
        &mut self,
        args: &[Vec<u8>],
        providers: Option<Providers>,
        bindings: galfus_contract::AdapterBindings,
        driver: std::rc::Rc<dyn galfus_runtime::driver::ExecutionDriver>,
    ) -> Result<galfus_contract::BoundaryValue, crate::state::WorkspaceRunError> {
        let mut execution =
            self.start_execution_with_bindings(args, providers, bindings, driver)?;
        execution
            .run_sync_to_completion()
            .map_err(crate::state::WorkspaceRunError::ExecutionFailed)
    }
}

impl galfus_bytecode::PackageLoader for Workspace {
    type Error = CompileBlocked;

    fn load(&mut self) -> Result<Arc<PackageImage>, Self::Error> {
        self.check();
        self.compile().map(|report| report.package)
    }
}
