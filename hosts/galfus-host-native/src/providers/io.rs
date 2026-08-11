use galfus_contract::builtins::std_io_provider_descriptor;
use galfus_contract::{
    BoundaryValue, CancellationOutcome, ExecutionFailure, ExecutionFailureKind, HostProvider,
    MessageInjector, ProviderDescriptor, TaskAffinity,
};
use std::sync::Arc;

pub struct NativeIoProvider;

impl HostProvider for NativeIoProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        std_io_provider_descriptor()
    }

    fn affinity(&self, _name: &str) -> TaskAffinity {
        // Manipulação de I/O bloqueante (stdout) deve ficar na Main
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
            "__provider_io_print" => {
                if let Some(BoundaryValue::Bytes(bytes)) = args.get(0) {
                    if let Ok(text) = std::str::from_utf8(bytes) {
                        println!("{}", text);
                    }
                }

                let _ = injector.inject_system_response(
                    thread_id,
                    request_lease,
                    Ok(BoundaryValue::Null),
                );
            }
            _ => {
                let _ = injector.inject_system_response(
                    thread_id,
                    request_lease,
                    Err(ExecutionFailure::new(
                        ExecutionFailureKind::ProviderFailure,
                        format!("Function {} not implemented in NativeIoProvider", name),
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
