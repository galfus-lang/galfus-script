use galfus_contract::builtins::std_time_provider_descriptor;
use galfus_contract::{
    BoundaryValue, CancellationOutcome, ExecutionFailure, ExecutionFailureKind, HostProvider,
    MessageInjector, ProviderDescriptor, TaskAffinity,
};
use std::sync::Arc;

pub struct WebTimeProvider;

impl WebTimeProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WebTimeProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl HostProvider for WebTimeProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        std_time_provider_descriptor()
    }

    fn affinity(&self, _name: &str) -> TaskAffinity {
        // `js_sys::Date::now()` pode ser chamado tanto da thread principal quanto de workers.
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
                // Em WebAssembly (wasm32-unknown-unknown), SystemTime::now() geralmente falha.
                // Usamos js_sys::Date::now() que interage diretamente com o JS.
                let ms = js_sys::Date::now() as i64;

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
                        format!("Function {} not implemented in WebTimeProvider", name),
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
