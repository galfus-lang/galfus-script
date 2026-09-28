use super::*;

use crate::state::*;
use crate::workspace::source_producer::WorkspaceSourceProducer;
use galfus_bytecode::PackageImage;
use galfus_runtime::{Execution, Runtime, SourceRuntimeConfiguration};
use std::sync::Arc;

impl Workspace {
    pub fn package_metadata(&self) -> galfus_bytecode::PackageMetadata {
        self.config
            .as_ref()
            .map(|config| galfus_bytecode::PackageMetadata {
                name: config.name().to_string(),
                version: config.version().map(String::from),
                author: config.author().map(String::from),
                email: config.email().map(String::from),
                description: config.description().map(String::from),
            })
            .unwrap_or_else(|| galfus_bytecode::PackageMetadata {
                name: "unknown".to_string(),
                version: None,
                author: None,
                email: None,
                description: None,
            })
    }

    /// Captures the current successful check as a source-backed lazy producer.
    #[allow(dead_code)]
    pub(crate) fn source_module_producer(&self) -> Option<Arc<WorkspaceSourceProducer>> {
        let catalog = self.semantic_state.module_catalog.as_ref()?;
        let snapshot = self.frontend_snapshot.as_ref()?;
        let CheckState::Passed {
            revision,
            semantic_revision,
            ..
        } = self.semantic_state.check_state
        else {
            return None;
        };
        (catalog.source_revision() == revision
            && catalog.semantic_revision() == semantic_revision
            && snapshot.semantic_revision() == semantic_revision)
            .then(|| {
                Arc::new(WorkspaceSourceProducer::new(
                    snapshot.clone(),
                    Arc::clone(catalog),
                    self.source_state.revision_guard(),
                    self.source_state.module_guard(),
                    self.timing_collector.clone(),
                ))
            })
    }

    pub fn start_execution(
        &mut self,
        args: &[Vec<u8>],
        providers: Option<Providers>,
        driver: std::rc::Rc<dyn galfus_runtime::driver::ExecutionDriver>,
    ) -> Result<Execution, crate::state::WorkspaceRunError> {
        self.start_source_execution(
            providers
                .map_or_else(RuntimeCapabilities::builder, |providers| {
                    RuntimeCapabilities::builder().with_providers(providers)
                })
                .build(),
            args,
            driver,
        )
    }

    pub fn start_execution_with_bindings(
        &mut self,
        args: &[Vec<u8>],
        providers: Option<Providers>,
        bindings: galfus_contract::AdapterBindings,
        driver: std::rc::Rc<dyn galfus_runtime::driver::ExecutionDriver>,
    ) -> Result<Execution, crate::state::WorkspaceRunError> {
        self.start_source_execution(
            providers
                .map_or_else(RuntimeCapabilities::builder, |providers| {
                    RuntimeCapabilities::builder().with_providers(providers)
                })
                .with_adapter_bindings(bindings)
                .build(),
            args,
            driver,
        )
    }

    /// Compatibility helper that drives the returned execution through the supplied driver.
    pub fn run(
        &mut self,
        args: &[Vec<u8>],
        providers: Option<Providers>,
        driver: std::rc::Rc<dyn galfus_runtime::driver::ExecutionDriver>,
    ) -> Result<i32, crate::state::WorkspaceRunError> {
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
    ) -> Result<i32, crate::state::WorkspaceRunError> {
        let mut execution =
            self.start_execution_with_bindings(args, providers, bindings, driver)?;
        execution
            .run_sync_to_completion()
            .map_err(crate::state::WorkspaceRunError::ExecutionFailed)
    }

    fn start_source_execution(
        &self,
        capabilities: RuntimeCapabilities,
        args: &[Vec<u8>],
        driver: std::rc::Rc<dyn galfus_runtime::driver::ExecutionDriver>,
    ) -> Result<Execution, crate::state::WorkspaceRunError> {
        // A workspace always executes through the source producer. A completed
        // package compilation is an artifact for a final host, never a reason
        // to change this boundary from lazy to eager materialization.
        let producer =
            self.source_module_producer()
                .ok_or(crate::state::WorkspaceRunError::Blocked(
                    RunBlocked::CheckRequired,
                ))?;
        let config = self
            .config
            .as_ref()
            .ok_or(crate::state::WorkspaceRunError::Blocked(
                RunBlocked::EntryModuleMissing,
            ))?;
        let entry_path =
            config
                .entry()
                .cloned()
                .ok_or(crate::state::WorkspaceRunError::Blocked(
                    RunBlocked::EntryModuleMissing,
                ))?;
        let catalog = self
            .module_catalog()
            .ok_or(crate::state::WorkspaceRunError::Blocked(
                RunBlocked::CheckRequired,
            ))?;
        let runtime_catalog =
            Arc::new(catalog.runtime_catalog().map_err(|error| {
                crate::state::WorkspaceRunError::SourceSetup(error.to_string())
            })?);
        let adapter_requirements = self.source_adapter_requirements_for(catalog);
        let provider_requirements = self
            .source_provider_requirements_for(catalog)
            .map_err(crate::state::WorkspaceRunError::SourceSetup)?;
        let configuration = SourceRuntimeConfiguration::new(
            runtime_catalog,
            galfus_bytecode::PackageEntryPoint::new(entry_path, config.run_entry()),
            config.limits().clone(),
            adapter_requirements,
            provider_requirements,
        );
        let started = std::time::Instant::now();
        let execution = Runtime::start_with_source_producer(
            configuration,
            producer,
            capabilities,
            args,
            driver,
        );
        if let Some(timing_collector) = &self.timing_collector {
            timing_collector.record_runtime_start(started.elapsed());
        }
        execution.map_err(crate::state::WorkspaceRunError::RuntimeStart)
    }
}

impl galfus_bytecode::PackageLoader for Workspace {
    type Error = CompileBlocked;

    fn load(&mut self) -> Result<Arc<PackageImage>, Self::Error> {
        self.check();
        self.compile().map(|report| report.package)
    }
}
