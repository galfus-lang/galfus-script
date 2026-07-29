use super::*;
use galfus_contract::ThreadExecutor;
use galfus_contract::{BoundaryValue, HostProvider, MessageInjector, Providers};
use galfus_runtime::SingleThreadExecutor;
use std::sync::{Arc, Mutex};

struct TerminatorIo {
    terminator: Arc<Mutex<Vec<u8>>>,
}

impl HostProvider for TerminatorIo {
    fn dispatch(
        &mut self,
        thread_id: usize,
        method: &str,
        args: &[BoundaryValue],
        injector: Arc<dyn MessageInjector>,
    ) {
        if method == "read" {
            if let Some(BoundaryValue::Bytes(terminator)) = args.first() {
                *self.terminator.lock().expect("terminator state") = terminator.clone();
            }
            injector.inject_system_response(thread_id, Ok(BoundaryValue::Bytes(Vec::new())));
        } else {
            injector.inject_system_response(thread_id, Ok(BoundaryValue::Null));
        }
    }
}

include!("tests/compilation.rs");
include!("tests/execution.rs");
