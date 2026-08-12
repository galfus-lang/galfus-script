use galfus_contract::builtins::std_env_provider_descriptor;
use galfus_contract::{
    BoundaryValue, CancellationOutcome, ExecutionFailure, ExecutionFailureKind, HostProvider,
    MessageInjector, ProviderDescriptor, TaskAffinity,
};
use std::sync::Arc;

pub struct NativeEnvProvider {
    metadata: galfus_bytecode::PackageMetadata,
}

impl NativeEnvProvider {
    pub fn new(metadata: galfus_bytecode::PackageMetadata) -> Self {
        Self { metadata }
    }
}

impl HostProvider for NativeEnvProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        std_env_provider_descriptor()
    }

    fn affinity(&self, _name: &str) -> TaskAffinity {
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
                let key = match args.get(0) {
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
                    _ => std::env::var(key).ok(),
                };

                if name == "env_has" {
                    let _ = injector.inject_system_response(
                        thread_id,
                        request_lease,
                        Ok(BoundaryValue::Bool(val.is_some())),
                    );
                } else {
                    let response = if let Some(s) = val {
                        BoundaryValue::Choice {
                            variant: 0,
                            payload: Some(Box::new(BoundaryValue::Bytes(s.into_bytes()))),
                        }
                    } else {
                        BoundaryValue::Choice {
                            variant: 1,
                            payload: Some(Box::new(BoundaryValue::Null)),
                        }
                    };

                    let _ = injector.inject_system_response(
                        thread_id,
                        request_lease,
                        Ok(response),
                    );
                }
            }
            _ => {
                let _ = injector.inject_system_response(
                    thread_id,
                    request_lease,
                    Err(ExecutionFailure::new(
                        ExecutionFailureKind::ProviderFailure,
                        format!("Function {} not implemented in NativeEnvProvider", name),
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
