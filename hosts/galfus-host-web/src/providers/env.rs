use galfus_contract::builtins::std_env_provider_descriptor;
use galfus_contract::{
    BoundaryValue, CancellationOutcome, ExecutionFailure, ExecutionFailureKind, HostProvider,
    MessageInjector, ProviderDescriptor, TaskAffinity,
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
        // As variáveis de ambiente já estão em memória (HashMap), pode rodar em qualquer thread.
        TaskAffinity::Any
    }

    fn dispatch(
        &mut self,
        thread_id: galfus_core::ThreadId,
        request_lease: galfus_core::RequestLease,
        name: &str,
        args: &[BoundaryValue],
        injector: Arc<dyn MessageInjector>,
    ) {
        match name {
            "env_get" | "env_has" => {
                let key = match args.first() {
                    Some(BoundaryValue::Bytes(bytes)) => match std::str::from_utf8(bytes) {
                        Ok(k) => k,
                        Err(_) => {
                            let _ = injector.inject_system_response(
                                thread_id,
                                request_lease,
                                Err(ExecutionFailure::new(
                                    ExecutionFailureKind::ProviderFailure,
                                    "Invalid UTF-8 key".to_string(),
                                )),
                            );
                            return;
                        }
                    },
                    _ => {
                        let _ = injector.inject_system_response(
                            thread_id,
                            request_lease,
                            Err(ExecutionFailure::new(
                                ExecutionFailureKind::ProviderFailure,
                                "Expected byte array for key".to_string(),
                            )),
                        );
                        return;
                    }
                };

                let val = match key {
                    "pkg.name" => Some(self.metadata.name.clone()),
                    "pkg.version" => self.metadata.version.clone(),
                    "pkg.author" => self.metadata.author.clone(),
                    "pkg.description" => self.metadata.description.clone(),
                    _ => self.env_vars.get(key).cloned(),
                };

                if name == "env_has" {
                    let _ = injector.inject_system_response(
                        thread_id,
                        request_lease,
                        Ok(BoundaryValue::Bool(val.is_some())),
                    );
                } else {
                    let response = val.map_or(BoundaryValue::Null, |value| {
                        BoundaryValue::Bytes(value.into_bytes())
                    });

                    let _ = injector.inject_system_response(thread_id, request_lease, Ok(response));
                }
            }
            _ => {
                let _ = injector.inject_system_response(
                    thread_id,
                    request_lease,
                    Err(ExecutionFailure::new(
                        ExecutionFailureKind::ProviderFailure,
                        format!("Function {} not implemented in WebEnvProvider", name),
                    )),
                );
            }
        }
    }

    fn cancel(
        &mut self,
        _thread_id: galfus_core::ThreadId,
        _request_lease: galfus_core::RequestLease,
    ) -> CancellationOutcome {
        CancellationOutcome::Unsupported
    }
}
