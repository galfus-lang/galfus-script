use galfus_contract::builtins::std_http_provider_descriptor;
use galfus_contract::{
    CancellationOutcome, ExecutionFailure, ExecutionFailureKind, HostProvider, MessageInjector,
    ProviderDescriptor, SurfaceValue, TaskAffinity,
};
use std::collections::HashMap;
use std::io::Read;
use std::sync::{Arc, Mutex};

type ResponseBody = Box<dyn Read + Send>;

pub struct NativeHttpProvider {
    next_body_id: u64,
    response_bodies: Arc<Mutex<HashMap<u64, ResponseBody>>>,
}
impl NativeHttpProvider {
    pub fn new() -> Self {
        Self {
            next_body_id: 1,
            response_bodies: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    fn register_body(&mut self, body: ResponseBody) -> u64 {
        let id = self.next_body_id;
        self.next_body_id = self.next_body_id.checked_add(1).unwrap_or(1);
        self.response_bodies.lock().unwrap().insert(id, body);
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
        if name != "http_request" || args.len() != 4 {
            return false;
        }
        let result = (|| -> Result<SurfaceValue, ExecutionFailure> {
            let method = String::from_utf8(bytes(&args[0], "method")?).map_err(|_| {
                ExecutionFailure::new(
                    ExecutionFailureKind::ProviderFailure,
                    "invalid UTF-8 method",
                )
            })?;
            let url = String::from_utf8(bytes(&args[1], "url")?).map_err(|_| {
                ExecutionFailure::new(ExecutionFailureKind::ProviderFailure, "invalid UTF-8 url")
            })?;
            let request = headers(&args[2])?
                .into_iter()
                .fold(ureq::request(&method, &url), |request, (name, value)| {
                    request.set(&name, &value)
                });
            let response = match &args[3] {
                SurfaceValue::Bytes(body) => request.send_bytes(body),
                SurfaceValue::Null => request.call(),
                _ => {
                    return Err(ExecutionFailure::new(
                        ExecutionFailureKind::ProviderFailure,
                        "expected nullable surface body",
                    ));
                }
            }
            .map_err(|_| {
                ExecutionFailure::new(ExecutionFailureKind::ProviderFailure, "HTTP request failed")
            })?;
            let status = response.status() as i32;
            let headers = response
                .headers_names()
                .into_iter()
                .filter_map(|name| {
                    response.header(&name).map(|value| {
                        SurfaceValue::Struct(vec![
                            ("name".to_string(), SurfaceValue::Bytes(name.into_bytes())),
                            (
                                "value".to_string(),
                                SurfaceValue::Bytes(value.as_bytes().to_vec()),
                            ),
                        ])
                    })
                })
                .collect();
            let body = self.register_body(response.into_reader());
            Ok(SurfaceValue::Struct(vec![
                ("status".to_string(), SurfaceValue::I32(status)),
                ("headers".to_string(), SurfaceValue::List(headers)),
                ("body".to_string(), SurfaceValue::U64(body)),
            ]))
        })();
        let _ = injector.inject_surface_response(thread_id, request_lease, result);
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
