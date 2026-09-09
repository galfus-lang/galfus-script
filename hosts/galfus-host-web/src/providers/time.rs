use galfus_contract::builtins::std_time_provider_descriptor;
use galfus_contract::{
    CancellationOutcome, HostProvider, MessageInjector, ProviderDescriptor, SurfaceValue,
    TaskAffinity,
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
        if name != "time_now" || !args.is_empty() {
            return false;
        }
        let _ = injector.inject_surface_response(
            thread_id,
            request_lease,
            Ok(SurfaceValue::I64(js_sys::Date::now() as i64)),
        );
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
