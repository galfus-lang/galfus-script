use galfus_contract::builtins::std_websocket_provider_descriptor;
use galfus_contract::{
    BoundaryValue, CancellationOutcome, ExecutionFailure, ExecutionFailureKind, HostProvider,
    MessageInjector, ProviderDescriptor, TaskAffinity,
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use wasm_bindgen::JsCast;

struct PendingReceive {
    thread_id: galfus_core::ThreadId,
    request_lease: galfus_core::RequestLease,
    injector: Arc<dyn MessageInjector>,
}

#[derive(Default)]
struct SharedState {
    closed: HashSet<u64>,
    messages: HashMap<u64, VecDeque<Vec<u8>>>,
    pending_receives: HashMap<u64, PendingReceive>,
}

pub struct WebWebSocketProvider {
    next_id: u64,
    sockets: HashMap<u64, web_sys::WebSocket>,
    state: Arc<Mutex<SharedState>>,
}

impl WebWebSocketProvider {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            sockets: HashMap::new(),
            state: Arc::new(Mutex::new(SharedState::default())),
        }
    }

    fn next_socket_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.checked_add(1).unwrap_or(1);
        id
    }
}

impl Default for WebWebSocketProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl HostProvider for WebWebSocketProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        std_websocket_provider_descriptor()
    }

    fn affinity(&self, _name: &str) -> TaskAffinity {
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
            "websocket_connect" => {
                let Some(BoundaryValue::Bytes(url)) = args.first() else {
                    let _ = injector.inject_system_response(
                        thread_id,
                        request_lease,
                        Err(ExecutionFailure::new(
                            ExecutionFailureKind::ProviderFailure,
                            "expected URL bytes".to_string(),
                        )),
                    );
                    return;
                };
                let Ok(url) = std::str::from_utf8(url) else {
                    let _ = injector.inject_system_response(
                        thread_id,
                        request_lease,
                        Err(ExecutionFailure::new(
                            ExecutionFailureKind::ProviderFailure,
                            "WebSocket URL must be UTF-8".to_string(),
                        )),
                    );
                    return;
                };
                let Ok(socket) = web_sys::WebSocket::new(url) else {
                    let _ = injector.inject_system_response(
                        thread_id,
                        request_lease,
                        Ok(BoundaryValue::Null),
                    );
                    return;
                };
                socket.set_binary_type(web_sys::BinaryType::Arraybuffer);
                let id = self.next_socket_id();
                let connected = Arc::new(std::sync::atomic::AtomicBool::new(false));
                let open_injector = injector.clone();
                let opened = connected.clone();
                let onopen = wasm_bindgen::closure::Closure::<dyn FnMut()>::new(move || {
                    if !opened.swap(true, std::sync::atomic::Ordering::SeqCst) {
                        let _ = open_injector.inject_system_response(
                            thread_id,
                            request_lease,
                            Ok(BoundaryValue::U64(id)),
                        );
                    }
                });
                socket.set_onopen(Some(onopen.as_ref().unchecked_ref()));
                onopen.forget();

                let error_injector = injector.clone();
                let failed = connected.clone();
                let onerror =
                    wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
                        if !failed.swap(true, std::sync::atomic::Ordering::SeqCst) {
                            let _ = error_injector.inject_system_response(
                                thread_id,
                                request_lease,
                                Ok(BoundaryValue::Null),
                            );
                        }
                    });
                socket.set_onerror(Some(onerror.as_ref().unchecked_ref()));
                onerror.forget();

                let state = self.state.clone();
                let onmessage =
                    wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::MessageEvent)>::new(
                        move |event: web_sys::MessageEvent| {
                            let data = event.data();
                            let bytes = if let Some(text) = data.as_string() {
                                text.into_bytes()
                            } else {
                                js_sys::Uint8Array::new(&data).to_vec()
                            };
                            let pending = state.lock().ok().and_then(|mut state| {
                                if let Some(pending) = state.pending_receives.remove(&id) {
                                    Some(pending)
                                } else {
                                    state
                                        .messages
                                        .entry(id)
                                        .or_default()
                                        .push_back(bytes.clone());
                                    None
                                }
                            });
                            if let Some(pending) = pending {
                                let _ = pending.injector.inject_system_response(
                                    pending.thread_id,
                                    pending.request_lease,
                                    Ok(BoundaryValue::Bytes(bytes)),
                                );
                            }
                        },
                    );
                socket.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
                onmessage.forget();
                let state = self.state.clone();
                let onclose = wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::CloseEvent)>::new(
                    move |_| {
                        if let Ok(mut state) = state.lock() {
                            state.closed.insert(id);
                            state.messages.remove(&id);
                            if let Some(pending) = state.pending_receives.remove(&id) {
                                let _ = pending.injector.inject_system_response(
                                    pending.thread_id,
                                    pending.request_lease,
                                    Ok(BoundaryValue::Null),
                                );
                            }
                        }
                    },
                );
                socket.set_onclose(Some(onclose.as_ref().unchecked_ref()));
                onclose.forget();
                if let Ok(mut state) = self.state.lock() {
                    state.closed.remove(&id);
                }
                self.sockets.insert(id, socket);
            }
            "websocket_receive" => {
                let Some(BoundaryValue::U64(id)) = args.first() else {
                    let _ = injector.inject_system_response(
                        thread_id,
                        request_lease,
                        Err(ExecutionFailure::new(
                            ExecutionFailureKind::ProviderFailure,
                            "expected socket ID".to_string(),
                        )),
                    );
                    return;
                };
                if !self.sockets.contains_key(id) {
                    let _ = injector.inject_system_response(
                        thread_id,
                        request_lease,
                        Ok(BoundaryValue::Null),
                    );
                    return;
                }
                let response = self.state.lock().ok().and_then(|mut state| {
                    if state.closed.contains(id) {
                        Some(None)
                    } else {
                        state
                            .messages
                            .get_mut(id)
                            .and_then(VecDeque::pop_front)
                            .map(Some)
                    }
                });
                if let Some(bytes) = response {
                    let _ = injector.inject_system_response(
                        thread_id,
                        request_lease,
                        Ok(bytes.map_or(BoundaryValue::Null, BoundaryValue::Bytes)),
                    );
                    return;
                }
                if let Ok(mut state) = self.state.lock() {
                    if state.closed.contains(id) {
                        let _ = injector.inject_system_response(
                            thread_id,
                            request_lease,
                            Ok(BoundaryValue::Null),
                        );
                        return;
                    }
                    state.pending_receives.insert(
                        *id,
                        PendingReceive {
                            thread_id,
                            request_lease,
                            injector,
                        },
                    );
                }
            }
            "websocket_send" => {
                let result = match (args.first(), args.get(1)) {
                    (Some(BoundaryValue::U64(id)), Some(BoundaryValue::Bytes(data))) => {
                        BoundaryValue::Bool(
                            self.sockets
                                .get(id)
                                .is_some_and(|socket| socket.send_with_u8_array(data).is_ok()),
                        )
                    }
                    _ => {
                        let _ = injector.inject_system_response(
                            thread_id,
                            request_lease,
                            Err(ExecutionFailure::new(
                                ExecutionFailureKind::ProviderFailure,
                                "expected socket ID and bytes".to_string(),
                            )),
                        );
                        return;
                    }
                };
                let _ = injector.inject_system_response(thread_id, request_lease, Ok(result));
            }
            "websocket_close" => {
                let Some(BoundaryValue::U64(id)) = args.first() else {
                    let _ = injector.inject_system_response(
                        thread_id,
                        request_lease,
                        Err(ExecutionFailure::new(
                            ExecutionFailureKind::ProviderFailure,
                            "expected socket ID".to_string(),
                        )),
                    );
                    return;
                };
                let closed = self
                    .sockets
                    .remove(id)
                    .is_some_and(|socket| socket.close().is_ok());
                if let Ok(mut state) = self.state.lock() {
                    state.closed.insert(*id);
                    state.messages.remove(id);
                    if let Some(pending) = state.pending_receives.remove(id) {
                        let _ = pending.injector.inject_system_response(
                            pending.thread_id,
                            pending.request_lease,
                            Ok(BoundaryValue::Null),
                        );
                    }
                }
                let _ = injector.inject_system_response(
                    thread_id,
                    request_lease,
                    Ok(BoundaryValue::Bool(closed)),
                );
            }
            _ => {
                let _ = injector.inject_system_response(
                    thread_id,
                    request_lease,
                    Err(ExecutionFailure::new(
                        ExecutionFailureKind::ProviderFailure,
                        format!("function {name} is not implemented in WebWebSocketProvider"),
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
