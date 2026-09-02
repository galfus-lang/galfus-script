use galfus_contract::builtins::std_io_provider_descriptor;
use galfus_contract::{
    BoundaryValue, CancellationOutcome, ExecutionFailure, ExecutionFailureKind, HostProvider,
    MessageInjector, ProviderDescriptor, SurfaceValue, TaskAffinity,
};
use std::io::Write;
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
            "io_write" => {
                if let Some(BoundaryValue::Bytes(bytes)) = args.first()
                    && let Ok(text) = std::str::from_utf8(bytes)
                {
                    print!("{}", text);
                    let _ = std::io::stdout().flush();
                }

                let _ = injector.inject_system_response(
                    thread_id,
                    request_lease,
                    Ok(BoundaryValue::Null),
                );
            }
            "io_read" => {
                let mut buffer = String::new();
                std::io::stdin().read_line(&mut buffer).unwrap();
                let _ = injector.inject_system_response(
                    thread_id,
                    request_lease,
                    Ok(BoundaryValue::Bytes(buffer.into_bytes())),
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

    fn dispatch_surface(
        &mut self,
        thread_id: galfus_core::ThreadId,
        request_lease: galfus_core::RequestLease,
        name: &str,
        args: &[SurfaceValue],
        injector: Arc<dyn MessageInjector>,
    ) -> bool {
        let result = match name {
            "io_write" => match args {
                [SurfaceValue::Bytes(bytes)] => {
                    if let Ok(text) = std::str::from_utf8(bytes) {
                        print!("{}", text);
                        let _ = std::io::stdout().flush();
                    }
                    Ok(SurfaceValue::Null)
                }
                _ => Err(ExecutionFailure::new(
                    ExecutionFailureKind::ProviderFailure,
                    "expected surface output bytes",
                )),
            },
            "io_read" => match args {
                [SurfaceValue::Bytes(_)] => {
                    let mut buffer = String::new();
                    std::io::stdin().read_line(&mut buffer).unwrap();
                    Ok(SurfaceValue::Bytes(buffer.into_bytes()))
                }
                _ => Err(ExecutionFailure::new(
                    ExecutionFailureKind::ProviderFailure,
                    "expected surface input terminator",
                )),
            },
            _ => Err(ExecutionFailure::new(
                ExecutionFailureKind::ProviderFailure,
                "unknown I/O operation",
            )),
        };
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
