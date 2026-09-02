use galfus_contract::builtins::std_env_provider_descriptor;
use galfus_contract::{
    CancellationOutcome, ExecutionFailure, ExecutionFailureKind, HostProvider, MessageInjector,
    ProviderDescriptor, SurfaceValue, TaskAffinity,
};
use std::collections::HashMap;
use std::sync::Arc;

pub struct WebEnvProvider {
    metadata: galfus_bytecode::PackageMetadata,
    env_vars: HashMap<String, String>,
}

impl WebEnvProvider {
    pub fn new(
        metadata: galfus_bytecode::PackageMetadata,
        env_vars: HashMap<String, String>,
    ) -> Self {
        Self { metadata, env_vars }
    }
}

impl HostProvider for WebEnvProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        std_env_provider_descriptor()
    }
    fn affinity(&self, _name: &str) -> TaskAffinity {
        TaskAffinity::Any
    }
    fn dispatch_surface(
        &mut self,
        thread_id: galfus_core::ThreadId,
        request_lease: galfus_core::RequestLease,
        name: &str,
        args: &[SurfaceValue],
        injector: Arc<dyn MessageInjector>,
    ) -> bool {
        let result = (|| -> Result<SurfaceValue, ExecutionFailure> {
            let [SurfaceValue::Bytes(key)] = args else {
                return Err(ExecutionFailure::new(
                    ExecutionFailureKind::ProviderFailure,
                    "expected surface environment key",
                ));
            };
            let key = std::str::from_utf8(key).map_err(|_| {
                ExecutionFailure::new(ExecutionFailureKind::ProviderFailure, "invalid UTF-8 key")
            })?;
            let value = match key {
                "pkg.name" => Some(self.metadata.name.clone()),
                "pkg.version" => self.metadata.version.clone(),
                "pkg.author" => self.metadata.author.clone(),
                "pkg.description" => self.metadata.description.clone(),
                _ => self.env_vars.get(key).cloned(),
            };
            match name {
                "env_get" => Ok(value.map_or(SurfaceValue::Null, |value| {
                    SurfaceValue::Bytes(value.into_bytes())
                })),
                "env_has" => Ok(SurfaceValue::Bool(value.is_some())),
                _ => Err(ExecutionFailure::new(
                    ExecutionFailureKind::ProviderFailure,
                    "unknown environment operation",
                )),
            }
        })();
        let _ = injector.inject_surface_response(thread_id, request_lease, result);
        true
    }
    fn cancel(
        &mut self,
        _thread_id: galfus_core::ThreadId,
        _request_lease: galfus_core::RequestLease,
    ) -> CancellationOutcome {
        CancellationOutcome::Unsupported
    }
}
