mod compilation;
mod execution;

use super::*;
use galfus_contract::KernelDriver;
use galfus_contract::{BoundaryType, BoundaryValue, HostProvider, MessageInjector, Providers};
use galfus_runtime::CooperativeDriver;
use std::sync::{Arc, Mutex};

struct TerminatorIo {
    terminator: Arc<Mutex<Vec<u8>>>,
}

impl HostProvider for TerminatorIo {
    fn descriptor(&self) -> galfus_contract::ProviderDescriptor {
        galfus_contract::std_io_provider_descriptor()
    }

    fn dispatch(
        &mut self,
        thread_id: galfus_core::ThreadId,
        request_lease: galfus_core::RequestLease,
        method: &str,
        args: &[BoundaryValue],
        injector: Arc<dyn MessageInjector>,
    ) {
        if method == "read" || method == "io_read" {
            if let Some(BoundaryValue::Array {
                element_type: BoundaryType::U8,
                values,
            }) = args.first()
            {
                *self.terminator.lock().expect("terminator state") = values
                    .iter()
                    .map(|value| match value {
                        BoundaryValue::U8(byte) => Some(*byte),
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>()
                    .expect("typed byte array");
            } else if let Some(BoundaryValue::Bytes(b)) = args.first() {
                *self.terminator.lock().expect("terminator state") = b.clone();
            }
            let _ = injector.inject_system_response(
                thread_id,
                request_lease,
                Ok(BoundaryValue::Bytes(Vec::new())),
            );
        } else {
            let _ =
                injector.inject_system_response(thread_id, request_lease, Ok(BoundaryValue::Null));
        }
    }
}
