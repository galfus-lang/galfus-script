use galfus_contract::builtins::std_io_provider_descriptor;
use galfus_contract::{
    BoundaryValue, CancellationOutcome, ExecutionFailure, ExecutionFailureKind, HostProvider,
    MessageInjector, ProviderDescriptor, TaskAffinity,
};
use std::sync::Arc;

pub struct WebIoProvider;

impl HostProvider for WebIoProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        std_io_provider_descriptor()
    }

    fn affinity(&self, _name: &str) -> TaskAffinity {
        // Interações com console do navegador e prompt costumam depender da thread principal no Web
        TaskAffinity::Main
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
            "io_write" => {
                if let Some(BoundaryValue::Bytes(bytes)) = args.get(0) {
                    if let Ok(text) = std::str::from_utf8(bytes) {
                        web_sys::console::log_1(&wasm_bindgen::JsValue::from_str(text));
                    }
                }

                let _ = injector.inject_system_response(
                    thread_id,
                    request_lease,
                    Ok(BoundaryValue::Null),
                );
            }
            "io_read" => {
                // Na Web, o prompt síncrono trava a UI ou exige async.
                // Como solicitado, sempre retorna string vazia.
                let _ = injector.inject_system_response(
                    thread_id,
                    request_lease,
                    Ok(BoundaryValue::Bytes(Vec::new())),
                );
            }
            _ => {
                let _ = injector.inject_system_response(
                    thread_id,
                    request_lease,
                    Err(ExecutionFailure::new(
                        ExecutionFailureKind::ProviderFailure,
                        format!("Function {} not implemented in WebIoProvider", name),
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
