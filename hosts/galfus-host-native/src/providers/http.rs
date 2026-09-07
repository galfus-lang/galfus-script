use galfus_contract::builtins::std_http_provider_descriptor;
use galfus_contract::{
    CancellationOutcome, ExecutionFailure, ExecutionFailureKind, HostProvider, MessageInjector,
    ProviderDescriptor, SurfaceValue, TaskAffinity,
};
use std::collections::HashMap;
use std::io::Read;
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Arc, Mutex};

type ResponseBody = Box<dyn Read + Send>;

struct ChannelReader {
    receiver: Receiver<Vec<u8>>,
    buffer: Vec<u8>,
}
impl Read for ChannelReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.buffer.is_empty() {
            match self.receiver.recv() {
                Ok(chunk) => self.buffer = chunk,
                Err(_) => return Ok(0),
            }
        }
        let len = std::cmp::min(buf.len(), self.buffer.len());
        buf[..len].copy_from_slice(&self.buffer[..len]);
        self.buffer = self.buffer[len..].to_vec();
        Ok(len)
    }
}

struct RequestSession {
    sender: Option<SyncSender<Vec<u8>>>,
    response_receiver: Receiver<Result<ureq::Response, ureq::Error>>,
}

pub struct NativeHttpProvider {
    next_id: u64,
    response_bodies: Arc<Mutex<HashMap<u64, ResponseBody>>>,
    active_requests: Arc<Mutex<HashMap<u64, RequestSession>>>,
}
impl NativeHttpProvider {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            response_bodies: Arc::new(Mutex::new(HashMap::new())),
            active_requests: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    fn register_request(&mut self, session: RequestSession) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.checked_add(1).unwrap_or(1);
        self.active_requests.lock().unwrap().insert(id, session);
        id
    }
}
impl Default for NativeHttpProvider {
    fn default() -> Self {
        Self::new()
    }
}

fn bytes(value: &SurfaceValue, name: &str) -> Result<Vec<u8>, ExecutionFailure> {
    match value {
        SurfaceValue::Bytes(value) => Ok(value.clone()),
        _ => Err(ExecutionFailure::new(
            ExecutionFailureKind::ProviderFailure,
            format!("expected surface bytes for {name}"),
        )),
    }
}
fn headers(value: &SurfaceValue) -> Result<Vec<(String, String)>, ExecutionFailure> {
    let SurfaceValue::List(headers) = value else {
        return Err(ExecutionFailure::new(
            ExecutionFailureKind::ProviderFailure,
            "expected surface header list",
        ));
    };
    headers
        .iter()
        .map(|header| {
            let SurfaceValue::Struct(fields) = header else {
                return Err(ExecutionFailure::new(
                    ExecutionFailureKind::ProviderFailure,
                    "expected surface header struct",
                ));
            };
            let field = |name| {
                fields
                    .iter()
                    .find_map(|(field, value)| (field == name).then_some(value))
                    .ok_or_else(|| {
                        ExecutionFailure::new(
                            ExecutionFailureKind::ProviderFailure,
                            format!("missing header {name}"),
                        )
                    })
            };
            let name = String::from_utf8(bytes(field("name")?, "header name")?).map_err(|_| {
                ExecutionFailure::new(ExecutionFailureKind::ProviderFailure, "invalid header name")
            })?;
            let value =
                String::from_utf8(bytes(field("value")?, "header value")?).map_err(|_| {
                    ExecutionFailure::new(
                        ExecutionFailureKind::ProviderFailure,
                        "invalid header value",
                    )
                })?;
            Ok((name, value))
        })
        .collect()
}

impl HostProvider for NativeHttpProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        std_http_provider_descriptor()
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
        if name == "http_response_read" {
            let [SurfaceValue::U64(id), SurfaceValue::U32(max)] = args else {
                return false;
            };
            if *max == 0 {
                return false;
            }
            let id = *id;
            let max = *max as usize;
            let body = self.response_bodies.lock().unwrap().remove(&id);
            let bodies = self.response_bodies.clone();
            std::thread::spawn(move || {
                let result = match body {
                    Some(mut body) => {
                        let mut bytes = vec![0; max];
                        match body.read(&mut bytes) {
                            Ok(0) | Err(_) => Ok(SurfaceValue::Null),
                            Ok(size) => {
                                bytes.truncate(size);
                                bodies.lock().unwrap().insert(id, body);
                                Ok(SurfaceValue::Bytes(bytes))
                            }
                        }
                    }
                    None => Ok(SurfaceValue::Null),
                };
                let _ = injector.inject_surface_response(thread_id, request_lease, result);
            });
            return true;
        }
        if name == "http_response_close" {
            let [SurfaceValue::U64(id)] = args else {
                return false;
            };
            let _ = injector.inject_surface_response(
                thread_id,
                request_lease,
                Ok(SurfaceValue::Bool(
                    self.response_bodies.lock().unwrap().remove(id).is_some(),
                )),
            );
            return true;
        }
        if name == "http_request_open" {
            let method = String::from_utf8(bytes(&args[0], "method").unwrap_or_default())
                .unwrap_or_default();
            let url =
                String::from_utf8(bytes(&args[1], "url").unwrap_or_default()).unwrap_or_default();
            let req_headers = headers(&args[2]).unwrap_or_default();

            let request = req_headers
                .into_iter()
                .fold(ureq::request(&method, &url), |request, (name, value)| {
                    request.set(&name, &value)
                });

            let (body_tx, body_rx) = sync_channel(8);
            let (res_tx, res_rx) = std::sync::mpsc::channel();

            std::thread::spawn(move || {
                let rx_reader = ChannelReader {
                    receiver: body_rx,
                    buffer: Vec::new(),
                };
                let response = request.send(rx_reader);
                let _ = res_tx.send(response);
            });

            let session = RequestSession {
                sender: Some(body_tx),
                response_receiver: res_rx,
            };
            let req_id = self.register_request(session);
            let _ = injector.inject_surface_response(
                thread_id,
                request_lease,
                Ok(SurfaceValue::U64(req_id)),
            );
            return true;
        }
        if name == "http_request_write" {
            let [SurfaceValue::U64(id), chunk] = args else {
                return false;
            };
            let chunk = bytes(chunk, "chunk").unwrap_or_default();
            let id = *id;

            let sender = self
                .active_requests
                .lock()
                .unwrap()
                .get(&id)
                .and_then(|s| s.sender.clone());
            std::thread::spawn(move || {
                let ok = if let Some(tx) = sender {
                    tx.send(chunk).is_ok()
                } else {
                    false
                };
                let _ = injector.inject_surface_response(
                    thread_id,
                    request_lease,
                    Ok(SurfaceValue::Bool(ok)),
                );
            });
            return true;
        }
        if name == "http_request_finish" {
            let [SurfaceValue::U64(id)] = args else {
                return false;
            };
            let id = *id;
            let req_data = self.active_requests.lock().unwrap().remove(&id);
            let bodies = self.response_bodies.clone();

            // Reserve body_id before spawning
            let body_id = self.next_id;
            self.next_id = self.next_id.checked_add(1).unwrap_or(1);

            std::thread::spawn(move || {
                let result = if let Some(mut session) = req_data {
                    session.sender.take();
                    match session.response_receiver.recv() {
                        Ok(Ok(response)) => {
                            let status = response.status() as i32;
                            let headers: Vec<_> = response
                                .headers_names()
                                .into_iter()
                                .filter_map(|name| {
                                    response.header(&name).map(|value| {
                                        SurfaceValue::Struct(vec![
                                            (
                                                "name".to_string(),
                                                SurfaceValue::Bytes(name.into_bytes()),
                                            ),
                                            (
                                                "value".to_string(),
                                                SurfaceValue::Bytes(value.as_bytes().to_vec()),
                                            ),
                                        ])
                                    })
                                })
                                .collect();
                            bodies
                                .lock()
                                .unwrap()
                                .insert(body_id, Box::new(response.into_reader()));
                            Ok(SurfaceValue::Struct(vec![
                                ("status".to_string(), SurfaceValue::I32(status)),
                                ("headers".to_string(), SurfaceValue::List(headers)),
                                ("body".to_string(), SurfaceValue::U64(body_id)),
                            ]))
                        }
                        err => {
                            println!("HTTP ERR: {:?}", err);
                            Ok(SurfaceValue::Null)
                        }
                    }
                } else {
                    Ok(SurfaceValue::Null)
                };
                let _ = injector.inject_surface_response(thread_id, request_lease, result);
            });
            return true;
        }
        if name == "http_request_abort" {
            let [SurfaceValue::U64(id)] = args else {
                return false;
            };
            let id = *id;
            let found = self.active_requests.lock().unwrap().remove(&id).is_some();
            let _ = injector.inject_surface_response(
                thread_id,
                request_lease,
                Ok(SurfaceValue::Bool(found)),
            );
            return true;
        }
        false
    }
    fn cancel(
        &mut self,
        _thread_id: galfus_core::ThreadId,
        _request_lease: galfus_core::RequestLease,
    ) -> CancellationOutcome {
        CancellationOutcome::Unsupported
    }
}
