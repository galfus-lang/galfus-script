pub mod io;
pub mod env;
pub mod time;
pub mod fs;

use galfus_bytecode::PackageMetadata;
use galfus_contract::{BoundaryValue, CancellationOutcome, HostProvider, MessageInjector, ProviderDescriptor, Providers, TaskAffinity};
use std::sync::Arc;

pub struct CompositeNativeProvider {
    io: io::NativeIoProvider,
    env: env::NativeEnvProvider,
    time: time::NativeTimeProvider,
    fs: fs::NativeFsProvider,
}

impl CompositeNativeProvider {
    pub fn new(metadata: PackageMetadata) -> Self {
        Self {
            io: io::NativeIoProvider,
            env: env::NativeEnvProvider::new(metadata),
            time: time::NativeTimeProvider::new(),
            fs: fs::NativeFsProvider::new(),
        }
    }
}

impl HostProvider for CompositeNativeProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        let mut modules = Vec::new();
        modules.extend(self.io.descriptor().modules);
        modules.extend(self.env.descriptor().modules);
        modules.extend(self.time.descriptor().modules);
        modules.extend(self.fs.descriptor().modules);
        ProviderDescriptor { modules }
    }

    fn affinity(&self, name: &str) -> TaskAffinity {
        if name.starts_with("io_") {
            self.io.affinity(name)
        } else if name.starts_with("env_") {
            self.env.affinity(name)
        } else if name.starts_with("time_") {
            self.time.affinity(name)
        } else if name.starts_with("fs_") {
            self.fs.affinity(name)
        } else {
            TaskAffinity::Any
        }
    }

    fn dispatch(
        &mut self,
        thread_id: galfus_core::ThreadId,
        request_lease: galfus_core::RequestLease,
        name: &str,
        args: &[BoundaryValue],
        injector: Arc<dyn MessageInjector>,
    ) {
        if name.starts_with("io_") {
            self.io.dispatch(thread_id, request_lease, name, args, injector);
        } else if name.starts_with("env_") {
            self.env.dispatch(thread_id, request_lease, name, args, injector);
        } else if name.starts_with("time_") {
            self.time.dispatch(thread_id, request_lease, name, args, injector);
        } else if name.starts_with("fs_") {
            self.fs.dispatch(thread_id, request_lease, name, args, injector);
        } else {
            // Function not found
            let _ = injector.inject_system_response(
                thread_id,
                request_lease,
                Err(galfus_contract::ExecutionFailure::new(
                    galfus_contract::ExecutionFailureKind::ProviderFailure,
                    format!("Function {} not implemented in CompositeNativeProvider", name),
                )),
            );
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

pub fn default_providers(metadata: PackageMetadata) -> Providers {
    Providers::with_host(Box::new(CompositeNativeProvider::new(metadata)))
}
