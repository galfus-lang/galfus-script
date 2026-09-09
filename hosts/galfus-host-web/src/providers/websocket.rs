use galfus_contract::builtins::std_websocket_provider_descriptor;
use galfus_contract::{
    CancellationOutcome, ExecutionFailure, ExecutionFailureKind, HostProvider, MessageInjector,
    ProviderDescriptor, SurfaceValue, TaskAffinity,
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

    fn dispatch_surface(
        &mut self,
        thread_id: galfus_core::ThreadId,
        request_lease: galfus_core::RequestLease,
        name: &str,
        args: &[SurfaceValue],
        injector: Arc<dyn MessageInjector>,
    ) -> bool {
        match name {
            "websocket_connect" => {
                let [SurfaceValue::Bytes(url)] = args else {
                    return false;
                };
                let Ok(url) = std::str::from_utf8(url) else {
                    let _ = injector.inject_surface_response(
                        thread_id,
                        request_lease,
                        Err(ExecutionFailure::new(
                            ExecutionFailureKind::ProviderFailure,
                            "WebSocket URL must be UTF-8",
                        )),
                    );
                    return true;
                };
                let Ok(socket) = web_sys::WebSocket::new(url) else {
                    let _ = injector.inject_surface_response(
                        thread_id,
                        request_lease,
                        Ok(SurfaceValue::Null),
                    );
                    return true;
                };
                socket.set_binary_type(web_sys::BinaryType::Arraybuffer);
                let id = self.next_socket_id();
                let completed = Arc::new(std::sync::atomic::AtomicBool::new(false));
                let opened = completed.clone();
                let open_injector = injector.clone();
                let onopen = wasm_bindgen::closure::Closure::<dyn FnMut()>::new(move || {
                    if !opened.swap(true, std::sync::atomic::Ordering::SeqCst) {
                        let _ = open_injector.inject_surface_response(
                            thread_id,
                            request_lease,
                            Ok(SurfaceValue::U64(id)),
                        );
                    }
                });
                socket.set_onopen(Some(onopen.as_ref().unchecked_ref()));
                onopen.forget();
                let failed = completed.clone();
                let error_injector = injector.clone();
                let onerror =
                    wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
                        if !failed.swap(true, std::sync::atomic::Ordering::SeqCst) {
                            let _ = error_injector.inject_surface_response(
                                thread_id,
                                request_lease,
                                Ok(SurfaceValue::Null),
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
                            let bytes = data.as_string().map_or_else(
                                || js_sys::Uint8Array::new(&data).to_vec(),
                                String::into_bytes,
                            );
                            let pending = state.lock().ok().and_then(|mut state| {
                                state.pending_receives.remove(&id).or_else(|| {
                                    state
                                        .messages
                                        .entry(id)
                                        .or_default()
                                        .push_back(bytes.clone());
                                    None
                                })
                            });
                            if let Some(pending) = pending {
                                let _ = pending.injector.inject_surface_response(
                                    pending.thread_id,
                                    pending.request_lease,
                                    Ok(SurfaceValue::Bytes(bytes)),
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
                                let _ = pending.injector.inject_surface_response(
                                    pending.thread_id,
                                    pending.request_lease,
                                    Ok(SurfaceValue::Null),
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
                true
            }
            "websocket_receive" => {
                let [SurfaceValue::U64(id)] = args else {
                    return false;
                };
                if !self.sockets.contains_key(id) {
                    let _ = injector.inject_surface_response(
                        thread_id,
                        request_lease,
                        Ok(SurfaceValue::Null),
                    );
                    return true;
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
                    let _ = injector.inject_surface_response(
                        thread_id,
                        request_lease,
                        Ok(bytes.map_or(SurfaceValue::Null, SurfaceValue::Bytes)),
                    );
                } else if let Ok(mut state) = self.state.lock() {
                    state.pending_receives.insert(
                        *id,
                        PendingReceive {
                            thread_id,
                            request_lease,
                            injector,
                        },
                    );
                }
                true
            }
            "websocket_send" => match args {
                [SurfaceValue::U64(id), SurfaceValue::Bytes(data)] => {
                    let result = SurfaceValue::Bool(
                        self.sockets
                            .get(id)
                            .is_some_and(|socket| socket.send_with_u8_array(data).is_ok()),
                    );
                    let _ = injector.inject_surface_response(thread_id, request_lease, Ok(result));
                    true
                }
                _ => false,
            },
            "websocket_close" => match args {
                [SurfaceValue::U64(id)] => {
                    let closed = self
                        .sockets
                        .remove(id)
                        .is_some_and(|socket| socket.close().is_ok());
                    if let Ok(mut state) = self.state.lock() {
                        state.closed.insert(*id);
                        state.messages.remove(id);
                        if let Some(pending) = state.pending_receives.remove(id) {
                            let _ = pending.injector.inject_surface_response(
                                pending.thread_id,
                                pending.request_lease,
                                Ok(SurfaceValue::Null),
                            );
                        }
                    }
                    let _ = injector.inject_surface_response(
                        thread_id,
                        request_lease,
                        Ok(SurfaceValue::Bool(closed)),
                    );
                    true
                }
                _ => false,
            },
            _ => false,
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
