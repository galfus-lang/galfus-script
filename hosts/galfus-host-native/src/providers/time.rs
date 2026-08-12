use galfus_contract::builtins::std_time_provider_descriptor;
use galfus_contract::{
    BoundaryValue, CancellationOutcome, ExecutionFailure, ExecutionFailureKind, HostProvider,
    MessageInjector, ProviderDescriptor, TaskAffinity,
};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct NativeTimeProvider;

impl NativeTimeProvider {
    pub fn new() -> Self {
        Self
    }
}

impl HostProvider for NativeTimeProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        std_time_provider_descriptor()
    }

    fn affinity(&self, _name: &str) -> TaskAffinity {
        TaskAffinity::Any
    }

    fn dispatch(
        &mut self,
        thread_id: galfus_core::ThreadId,
        request_lease: galfus_core::RequestLease,
        name: &str,
        _args: &[BoundaryValue],
        injector: Arc<dyn MessageInjector>,
    ) {
        match name {
            "time_now" => {
                let ms = match SystemTime::now().duration_since(UNIX_EPOCH) {
                    Ok(d) => d.as_millis() as i64,
                    Err(_) => 0,
                };
                let _ = injector.inject_system_response(
                    thread_id,
                    request_lease,
                    Ok(BoundaryValue::I64(ms)),
                );
            }
            _ => {
                let _ = injector.inject_system_response(
                    thread_id,
                    request_lease,
                    Err(ExecutionFailure::new(
                        ExecutionFailureKind::ProviderFailure,
                        format!("Function {} not implemented in NativeTimeProvider", name),
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
