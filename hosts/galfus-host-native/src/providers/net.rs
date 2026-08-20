use galfus_contract::builtins::std_net_provider_descriptor;
use galfus_contract::{
    BoundaryValue, CancellationOutcome, ExecutionFailure, ExecutionFailureKind, HostProvider,
    MessageInjector, ProviderDescriptor, TaskAffinity,
};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpStream, UdpSocket};
use std::sync::Arc;

pub struct NativeNetProvider {
    next_id: u64,
    tcp: HashMap<u64, TcpStream>,
    udp: HashMap<u64, UdpSocket>,
}

impl Default for NativeNetProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl NativeNetProvider {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            tcp: HashMap::new(),
            udp: HashMap::new(),
        }
    }

    fn next_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.checked_add(1).unwrap_or(1);
        id
    }

    fn text(args: &[BoundaryValue], index: usize, label: &str) -> Result<String, ExecutionFailure> {
        match args.get(index) {
            Some(BoundaryValue::Bytes(value)) => String::from_utf8(value.clone()).map_err(|_| {
                ExecutionFailure::new(
                    ExecutionFailureKind::ProviderFailure,
                    format!("invalid UTF-8 {label}"),
                )
            }),
            _ => Err(ExecutionFailure::new(
                ExecutionFailureKind::ProviderFailure,
                format!("expected bytes for {label}"),
            )),
        }
    }

    fn u16(args: &[BoundaryValue], index: usize, label: &str) -> Result<u16, ExecutionFailure> {
        match args.get(index) {
            Some(BoundaryValue::U16(value)) => Ok(*value),
            _ => Err(ExecutionFailure::new(
                ExecutionFailureKind::ProviderFailure,
                format!("expected u16 for {label}"),
            )),
        }
    }

    fn u64(args: &[BoundaryValue], index: usize, label: &str) -> Result<u64, ExecutionFailure> {
        match args.get(index) {
            Some(BoundaryValue::U64(value)) => Ok(*value),
            _ => Err(ExecutionFailure::new(
                ExecutionFailureKind::ProviderFailure,
                format!("expected u64 for {label}"),
            )),
        }
    }

    fn max_bytes(args: &[BoundaryValue], index: usize) -> Result<usize, ExecutionFailure> {
        match args.get(index) {
            Some(BoundaryValue::U32(value)) => Ok((*value).clamp(1, 1_048_576) as usize),
            _ => Err(ExecutionFailure::new(
                ExecutionFailureKind::ProviderFailure,
                "expected u32 for max_bytes".to_string(),
            )),
        }
    }

    fn reply(
        injector: Arc<dyn MessageInjector>,
        thread_id: galfus_core::ThreadId,
        request_lease: galfus_core::RequestLease,
        value: Result<BoundaryValue, ExecutionFailure>,
    ) {
        let _ = injector.inject_system_response(thread_id, request_lease, value);
    }
}

impl HostProvider for NativeNetProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        std_net_provider_descriptor()
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
        let value = match name {
            "net_tcp_connect" => {
                let host = Self::text(args, 0, "host");
                let port = Self::u16(args, 1, "port");
                match host.and_then(|host| port.map(|port| (host, port))) {
                    Ok((host, port)) => match TcpStream::connect((host.as_str(), port)) {
                        Ok(stream) => {
                            let id = self.next_id();
                            self.tcp.insert(id, stream);
                            Ok(BoundaryValue::U64(id))
                        }
                        Err(_) => Ok(BoundaryValue::Null),
                    },
                    Err(error) => Err(error),
                }
            }
            "net_tcp_read" => {
                let id = Self::u64(args, 0, "socket");
                let max = Self::max_bytes(args, 1);
                match id.and_then(|id| max.map(|max| (id, max))) {
                    Ok((id, max)) => {
                        match self.tcp.get(&id).and_then(|stream| stream.try_clone().ok()) {
                            Some(mut stream) => {
                                std::thread::spawn(move || {
                                    let mut buffer = vec![0; max];
                                    let result = match stream.read(&mut buffer) {
                                        Ok(0) | Err(_) => Ok(BoundaryValue::Null),
                                        Ok(size) => {
                                            buffer.truncate(size);
                                            Ok(BoundaryValue::Bytes(buffer))
                                        }
                                    };
                                    Self::reply(injector, thread_id, request_lease, result);
                                });
                                return;
                            }
                            None => Ok(BoundaryValue::Null),
                        }
                    }
                    Err(error) => Err(error),
                }
            }
            "net_tcp_write" => {
                let id = Self::u64(args, 0, "socket");
                let data = match args.get(1) {
                    Some(BoundaryValue::Bytes(value)) => Ok(value),
                    _ => Err(ExecutionFailure::new(
                        ExecutionFailureKind::ProviderFailure,
                        "expected bytes for data".to_string(),
                    )),
                };
                match id.and_then(|id| data.map(|data| (id, data))) {
                    Ok((id, data)) => Ok(BoundaryValue::Bool(
                        self.tcp
                            .get_mut(&id)
                            .is_some_and(|stream| stream.write_all(data).is_ok()),
                    )),
                    Err(error) => Err(error),
                }
            }
            "net_tcp_close" => match Self::u64(args, 0, "socket") {
                Ok(id) => Ok(BoundaryValue::Bool(self.tcp.remove(&id).is_some())),
                Err(error) => Err(error),
            },
            "net_udp_bind" => {
                let host = Self::text(args, 0, "host");
                let port = Self::u16(args, 1, "port");
                match host.and_then(|host| port.map(|port| (host, port))) {
                    Ok((host, port)) => match UdpSocket::bind((host.as_str(), port)) {
                        Ok(socket) => {
                            let id = self.next_id();
                            self.udp.insert(id, socket);
                            Ok(BoundaryValue::U64(id))
                        }
                        Err(_) => Ok(BoundaryValue::Null),
                    },
                    Err(error) => Err(error),
                }
            }
            "net_udp_receive" => {
                let id = Self::u64(args, 0, "socket");
                let max = Self::max_bytes(args, 1);
                match id.and_then(|id| max.map(|max| (id, max))) {
                    Ok((id, max)) => {
                        match self.udp.get(&id).and_then(|socket| socket.try_clone().ok()) {
                            Some(socket) => {
                                std::thread::spawn(move || {
                                    let mut buffer = vec![0; max];
                                    let result = match socket.recv_from(&mut buffer) {
                                        Ok((size, peer)) => {
                                            buffer.truncate(size);
                                            Ok(BoundaryValue::Tuple(vec![
                                                BoundaryValue::Bytes(buffer),
                                                BoundaryValue::Bytes(
                                                    peer.ip().to_string().into_bytes(),
                                                ),
                                                BoundaryValue::U16(peer.port()),
                                            ]))
                                        }
                                        Err(_) => Ok(BoundaryValue::Null),
                                    };
                                    Self::reply(injector, thread_id, request_lease, result);
                                });
                                return;
                            }
                            None => Ok(BoundaryValue::Null),
                        }
                    }
                    Err(error) => Err(error),
                }
            }
            "net_udp_send_to" => {
                let id = Self::u64(args, 0, "socket");
                let host = Self::text(args, 1, "host");
                let port = Self::u16(args, 2, "port");
                let data = match args.get(3) {
                    Some(BoundaryValue::Bytes(value)) => Ok(value),
                    _ => Err(ExecutionFailure::new(
                        ExecutionFailureKind::ProviderFailure,
                        "expected bytes for data".to_string(),
                    )),
                };
                match id.and_then(|id| Ok((id, host?, port?, data?))) {
                    Ok((id, host, port, data)) => {
                        Ok(BoundaryValue::Bool(self.udp.get(&id).is_some_and(
                            |socket| socket.send_to(data, (host.as_str(), port)).is_ok(),
                        )))
                    }
                    Err(error) => Err(error),
                }
            }
            "net_udp_close" => match Self::u64(args, 0, "socket") {
                Ok(id) => Ok(BoundaryValue::Bool(self.udp.remove(&id).is_some())),
                Err(error) => Err(error),
            },
            _ => Err(ExecutionFailure::new(
                ExecutionFailureKind::ProviderFailure,
                format!("function {name} is not implemented in NativeNetProvider"),
            )),
        };
        Self::reply(injector, thread_id, request_lease, value);
    }

    fn cancel(
        &mut self,
        _thread_id: galfus_core::ThreadId,
        _request_lease: galfus_core::RequestLease,
    ) -> CancellationOutcome {
        CancellationOutcome::Unsupported
    }
}
