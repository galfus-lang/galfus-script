#[cfg(test)]
mod tests;

use std::mem;

use galfus_contract::{
    BoundaryType, BoundaryValue, ExecutionFailure, ExecutionFailureKind, HostProvider,
    MessageInjector,
};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[cfg(feature = "wasm")]
use js_sys::{Function, Uint8Array};
#[cfg(feature = "wasm")]
use wasm_bindgen::JsValue;

/// In-memory synchronous I/O for playground hosts and tests.
#[derive(Clone, Default)]
pub struct BufferIoProvider {
    state: Arc<Mutex<BufferIoState>>,
}

#[derive(Default)]
struct BufferIoState {
    input: VecDeque<u8>,
    output: Vec<u8>,
    pending_read: Option<(usize, u64, Vec<u8>, Arc<dyn MessageInjector>)>,
    #[cfg(feature = "wasm")]
    write_callback: Option<WriteCallback>,
}

fn boundary_bytes(value: &BoundaryValue) -> Option<Vec<u8>> {
    match value {
        BoundaryValue::Bytes(bytes) => Some(bytes.clone()),
        BoundaryValue::Array {
            element_type: BoundaryType::U8,
            values,
        } => values
            .iter()
            .map(|value| match value {
                BoundaryValue::U8(byte) => Some(*byte),
                _ => None,
            })
            .collect(),
        _ => None,
    }
}

#[cfg(feature = "wasm")]
#[derive(Clone)]
struct WriteCallback(Function);

#[cfg(feature = "wasm")]
// The playground executes synchronously on the browser's single thread.
unsafe impl Send for WriteCallback {}

impl BufferIoProvider {
    pub fn new(input: impl Into<Vec<u8>>) -> Self {
        Self {
            state: Arc::new(Mutex::new(BufferIoState {
                input: input.into().into(),
                output: Vec::new(),
                pending_read: None,
                #[cfg(feature = "wasm")]
                write_callback: None,
            })),
        }
    }

    pub fn take_output(&self) -> Vec<u8> {
        mem::take(&mut self.state.lock().expect("buffer I/O state").output)
    }

    pub fn send_read_data(&self, bytes: &[u8]) {
        let mut state = self.state.lock().expect("buffer I/O state");
        state.input.extend(bytes);

        if let Some((thread_id, request_id, terminator, injector)) = state.pending_read.take() {
            let mut input = Vec::new();
            let mut found = false;
            for &byte in state.input.iter() {
                input.push(byte);
                if input.ends_with(&terminator) {
                    found = true;
                    break;
                }
            }
            if found {
                let len = input.len();
                state.input.drain(0..len);
                input.truncate(len - terminator.len());
                injector.inject_system_response(
                    thread_id,
                    request_id,
                    Ok(BoundaryValue::Bytes(input)),
                );
            } else {
                state.pending_read = Some((thread_id, request_id, terminator, injector));
            }
        }
    }

    #[cfg(feature = "wasm")]
    pub fn set_write_callback(&self, callback: Function) {
        self.state.lock().expect("buffer I/O state").write_callback = Some(WriteCallback(callback));
    }
}

impl HostProvider for BufferIoProvider {
    fn dispatch(
        &mut self,
        thread_id: usize,
        request_id: u64,
        method: &str,
        args: &[BoundaryValue],
        injector: Arc<dyn MessageInjector>,
    ) {
        match method {
            "io_write" => {
                if let Some(bytes) = args.first().and_then(boundary_bytes) {
                    #[cfg(feature = "wasm")]
                    let callback = {
                        let mut state = self.state.lock().expect("buffer I/O state");
                        state.output.extend_from_slice(&bytes);
                        state.write_callback.clone()
                    };

                    #[cfg(not(feature = "wasm"))]
                    self.state
                        .lock()
                        .expect("buffer I/O state")
                        .output
                        .extend_from_slice(&bytes);

                    #[cfg(feature = "wasm")]
                    if let Some(WriteCallback(callback)) = callback {
                        let value = Uint8Array::from(bytes.as_slice());
                        if let Err(e) = callback.call1(&JsValue::UNDEFINED, &value.into()) {
                            injector.inject_system_response(
                                thread_id,
                                request_id,
                                Err(ExecutionFailure::new(
                                    ExecutionFailureKind::ProviderFailure,
                                    format!("{:?}", e),
                                )),
                            );
                            return;
                        }
                    }
                    injector.inject_system_response(thread_id, request_id, Ok(BoundaryValue::Null));
                } else {
                    injector.inject_system_response(
                        thread_id,
                        request_id,
                        Err(ExecutionFailure::new(
                            ExecutionFailureKind::ProviderFailure,
                            "Invalid arguments for write".to_string(),
                        )),
                    );
                }
            }
            "io_read" => {
                let terminator = if let Some(bytes) = args.first().and_then(boundary_bytes) {
                    bytes
                } else {
                    injector.inject_system_response(
                        thread_id,
                        request_id,
                        Err(ExecutionFailure::new(
                            ExecutionFailureKind::ProviderFailure,
                            "Invalid arguments for read".to_string(),
                        )),
                    );
                    return;
                };

                if terminator.is_empty() {
                    injector.inject_system_response(
                        thread_id,
                        request_id,
                        Err(ExecutionFailure::new(
                            ExecutionFailureKind::ProviderFailure,
                            "input terminator must not be empty".to_string(),
                        )),
                    );
                    return;
                }

                let mut state = self.state.lock().expect("buffer I/O state");
                let mut input = Vec::new();
                let mut found = false;
                for &byte in state.input.iter() {
                    input.push(byte);
                    if input.ends_with(&terminator) {
                        found = true;
                        break;
                    }
                }

                if found {
                    let len = input.len();
                    state.input.drain(0..len);
                    input.truncate(len - terminator.len());
                    injector.inject_system_response(
                        thread_id,
                        request_id,
                        Ok(BoundaryValue::Bytes(input)),
                    );
                } else {
                    state.pending_read = Some((thread_id, request_id, terminator, injector));
                }
            }
            _ => {
                injector.inject_system_response(
                    thread_id,
                    request_id,
                    Err(ExecutionFailure::new(
                        ExecutionFailureKind::ProviderFailure,
                        format!("Method {} not found", method),
                    )),
                );
            }
        }
    }
}
