use galfus_contract::builtins::std_websocket_provider_descriptor;
use galfus_contract::{
    BoundaryValue, CancellationOutcome, ExecutionFailure, ExecutionFailureKind, HostProvider,
    MessageInjector, ProviderDescriptor, TaskAffinity,
};
use std::collections::HashMap;
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use tungstenite::{Message, WebSocket, stream::MaybeTlsStream};

pub struct NativeWebSocketProvider {
    next_id: u64,
    sockets: HashMap<u64, Arc<Mutex<WebSocket<MaybeTlsStream<TcpStream>>>>>,
}
impl NativeWebSocketProvider {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            sockets: HashMap::new(),
        }
    }
}
impl Default for NativeWebSocketProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl HostProvider for NativeWebSocketProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        std_websocket_provider_descriptor()
    }
    fn affinity(&self, _name: &str) -> TaskAffinity {
        TaskAffinity::Any
    }
    fn dispatch(
        &mut self,
        thread_id: galfus_core::ThreadId,
        request_lease: galfus_core::RequestLease,
        name: &str,
        args: &[BoundaryValue],
        injector: Arc<dyn MessageInjector>,
    ) {
        let result = match name {
            "websocket_connect" => match args.first() {
                Some(BoundaryValue::Bytes(url)) => match String::from_utf8(url.clone())
                    .ok()
                    .and_then(|url| tungstenite::connect(url).ok())
                {
                    Some((socket, _)) => {
                        let id = self.next_id;
                        self.next_id = self.next_id.checked_add(1).unwrap_or(1);
                        self.sockets.insert(id, Arc::new(Mutex::new(socket)));
                        Ok(BoundaryValue::U64(id))
                    }
                    None => Ok(BoundaryValue::Null),
                },
                _ => Err(ExecutionFailure::new(
                    ExecutionFailureKind::ProviderFailure,
                    "expected URL bytes".to_string(),
                )),
            },
            "websocket_receive" => match args.first() {
                Some(BoundaryValue::U64(id)) => match self.sockets.get(id).cloned() {
                    Some(socket) => {
                        std::thread::spawn(move || {
                            let result = match socket
                                .lock()
                                .ok()
                                .and_then(|mut socket| socket.read().ok())
                            {
                                Some(Message::Binary(data)) => Ok(BoundaryValue::Bytes(data)),
                                Some(Message::Text(text)) => {
                                    Ok(BoundaryValue::Bytes(text.into_bytes()))
                                }
                                _ => Ok(BoundaryValue::Null),
                            };
                            let _ =
                                injector.inject_system_response(thread_id, request_lease, result);
                        });
                        return;
                    }
                    None => Ok(BoundaryValue::Null),
                },
                _ => Err(ExecutionFailure::new(
                    ExecutionFailureKind::ProviderFailure,
                    "expected socket ID".to_string(),
                )),
            },
            "websocket_send" => match (args.first(), args.get(1)) {
                (Some(BoundaryValue::U64(id)), Some(BoundaryValue::Bytes(data))) => Ok(
                    BoundaryValue::Bool(self.sockets.get(id).is_some_and(|socket| {
                        socket.lock().is_ok_and(|mut socket| {
                            socket.send(Message::Binary(data.clone())).is_ok()
                        })
                    })),
                ),
                _ => Err(ExecutionFailure::new(
                    ExecutionFailureKind::ProviderFailure,
                    "expected socket ID and bytes".to_string(),
                )),
            },
            "websocket_close" => match args.first() {
                Some(BoundaryValue::U64(id)) => match self.sockets.remove(id) {
                    Some(socket) => {
                        let _ = socket.lock().map(|mut socket| socket.close(None));
                        Ok(BoundaryValue::Bool(true))
                    }
                    None => Ok(BoundaryValue::Bool(false)),
                },
                _ => Err(ExecutionFailure::new(
                    ExecutionFailureKind::ProviderFailure,
                    "expected socket ID".to_string(),
                )),
            },
            _ => Err(ExecutionFailure::new(
                ExecutionFailureKind::ProviderFailure,
                format!("function {name} is not implemented in NativeWebSocketProvider"),
            )),
        };
        let _ = injector.inject_system_response(thread_id, request_lease, result);
    }
    fn cancel(
        &mut self,
        _thread_id: galfus_core::ThreadId,
        _request_lease: galfus_core::RequestLease,
    ) -> CancellationOutcome {
        CancellationOutcome::Unsupported
    }
}
