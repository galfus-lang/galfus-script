use super::*;
use galfus_contract::KernelDriver;
use galfus_contract::{BoundaryType, BoundaryValue, HostProvider, MessageInjector, Providers};
use galfus_runtime::CooperativeDriver;
use std::sync::{Arc, Mutex};

struct TerminatorIo {
    terminator: Arc<Mutex<Vec<u8>>>,
}

impl HostProvider for TerminatorIo {
    fn dispatch(
        &mut self,
        thread_id: usize,
        request_id: u64,
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
            injector.inject_system_response(
                thread_id,
                request_id,
                Ok(BoundaryValue::Bytes(Vec::new())),
            );
        } else {
            injector.inject_system_response(thread_id, request_id, Ok(BoundaryValue::Null));
        }
    }
}

include!("tests/compilation.rs");
include!("tests/execution.rs");
