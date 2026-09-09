use galfus_contract::builtins::std_http_provider_descriptor;
use galfus_contract::{
    CancellationOutcome, ExecutionFailure, ExecutionFailureKind, HostProvider, MessageInjector,
    ProviderDescriptor, SurfaceValue, TaskAffinity,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

struct ActiveRequest {
    writer: Option<web_sys::WritableStreamDefaultWriter>,
    fetch_promise: js_sys::Promise,
}

thread_local! {
    static RESPONSE_BODIES: RefCell<HashMap<u64, web_sys::ReadableStreamDefaultReader>> = RefCell::new(HashMap::new());
    static ACTIVE_REQUESTS: RefCell<HashMap<u64, ActiveRequest>> = RefCell::new(HashMap::new());
}
static NEXT_BODY_ID: AtomicU64 = AtomicU64::new(1);

pub struct WebHttpProvider;
impl WebHttpProvider {
    pub fn new() -> Self {
        Self
    }
    fn next_body_id() -> u64 {
        NEXT_BODY_ID.fetch_add(1, Ordering::Relaxed)
    }
}
impl Default for WebHttpProvider {
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

impl HostProvider for WebHttpProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        std_http_provider_descriptor()
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
        if name == "http_response_read" {
            let [SurfaceValue::U64(id), SurfaceValue::U32(max)] = args else {
                return false;
            };
            if *max == 0 {
                return false;
            }
            let id = *id;
            let reader = RESPONSE_BODIES.with(|b| b.borrow_mut().remove(&id));
            wasm_bindgen_futures::spawn_local(async move {
                let result = match reader {
                    Some(reader) => {
                        let promise = reader.read();
                        let promise = JsFuture::from(promise);
                        match promise.await {
                            Ok(chunk) => {
                                let chunk_obj = chunk.unchecked_into::<js_sys::Object>();
                                let done =
                                    js_sys::Reflect::get(&chunk_obj, &"done".into()).unwrap();
                                if done.is_truthy() {
                                    Ok(SurfaceValue::Null)
                                } else {
                                    let value =
                                        js_sys::Reflect::get(&chunk_obj, &"value".into()).unwrap();
                                    let array = js_sys::Uint8Array::new(&value);
                                    let mut bytes = vec![0; array.length() as usize];
                                    array.copy_to(&mut bytes);
                                    RESPONSE_BODIES.with(|b| b.borrow_mut().insert(id, reader));
                                    Ok(SurfaceValue::Bytes(bytes))
                                }
                            }
                            Err(_) => Ok(SurfaceValue::Null),
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
            let removed = RESPONSE_BODIES.with(|b| b.borrow_mut().remove(id).is_some());
            let _ = injector.inject_surface_response(
                thread_id,
                request_lease,
                Ok(SurfaceValue::Bool(removed)),
            );
            return true;
        }

        if name == "http_request_open" {
            let method = String::from_utf8(bytes(&args[0], "method").unwrap_or_default())
                .unwrap_or_default();
            let url =
                String::from_utf8(bytes(&args[1], "url").unwrap_or_default()).unwrap_or_default();
            let req_headers = headers(&args[2]).unwrap_or_default();

            let opts = web_sys::RequestInit::new();
            opts.set_method(&method);
            opts.set_mode(web_sys::RequestMode::Cors);
            let headers_obj = web_sys::Headers::new().unwrap();
            for (name, value) in req_headers {
                let _ = headers_obj.append(&name, &value);
            }
            opts.set_headers(&headers_obj);

            let req_id = Self::next_body_id();
            let transform = web_sys::TransformStream::new().unwrap();
            opts.set_body(&transform.readable());
            let writer = transform.writable().get_writer().unwrap();

            let request = web_sys::Request::new_with_str_and_init(&url, &opts).unwrap();
            let global = js_sys::global();
            let fetch_fn = js_sys::Reflect::get(&global, &"fetch".into()).unwrap();
            let fetch_fn = fetch_fn.dyn_into::<js_sys::Function>().unwrap();
            let fetch_promise = fetch_fn
                .call1(&global, &request)
                .unwrap()
                .dyn_into::<js_sys::Promise>()
                .unwrap();

            ACTIVE_REQUESTS.with(|b| {
                b.borrow_mut().insert(
                    req_id,
                    ActiveRequest {
                        writer: Some(writer),
                        fetch_promise,
                    },
                );
            });

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

            let writer =
                ACTIVE_REQUESTS.with(|b| b.borrow().get(&id).and_then(|r| r.writer.clone()));

            wasm_bindgen_futures::spawn_local(async move {
                let ok = if let Some(w) = writer {
                    let array = js_sys::Uint8Array::from(&chunk[..]);
                    let promise = w.write_with_chunk(&array);
                    JsFuture::from(promise).await.is_ok()
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
            let req_data = ACTIVE_REQUESTS.with(|b| b.borrow_mut().remove(&id));

            wasm_bindgen_futures::spawn_local(async move {
                let result = if let Some(mut r) = req_data {
                    if let Some(w) = r.writer.take() {
                        let _ = JsFuture::from(w.close()).await;
                    }
                    match JsFuture::from(r.fetch_promise).await {
                        Ok(resp) => {
                            let resp: web_sys::Response = resp.dyn_into().unwrap();
                            let status = resp.status() as i32;
                            let headers = resp.headers();
                            let mut rust_headers = vec![];
                            // ... web_sys Headers iterator is annoying, let's keep it simple or use array entries
                            let headers_iter = js_sys::try_iter(&headers).unwrap().unwrap();
                            for entry in headers_iter {
                                let entry = entry.unwrap();
                                let array = entry.unchecked_into::<js_sys::Array>();
                                let name = array.get(0).as_string().unwrap();
                                let value = array.get(1).as_string().unwrap();
                                rust_headers.push(SurfaceValue::Struct(vec![
                                    ("name".to_string(), SurfaceValue::Bytes(name.into_bytes())),
                                    ("value".to_string(), SurfaceValue::Bytes(value.into_bytes())),
                                ]));
                            }

                            let body_id = Self::next_body_id();
                            if let Some(stream) = resp.body() {
                                let reader = stream.get_reader();
                                let reader: web_sys::ReadableStreamDefaultReader =
                                    reader.unchecked_into();
                                RESPONSE_BODIES.with(|b| b.borrow_mut().insert(body_id, reader));
                            }
                            Ok(SurfaceValue::Struct(vec![
                                ("status".to_string(), SurfaceValue::I32(status)),
                                ("headers".to_string(), SurfaceValue::List(rust_headers)),
                                ("body".to_string(), SurfaceValue::U64(body_id)),
                            ]))
                        }
                        Err(_) => Ok(SurfaceValue::Null),
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
            let found = ACTIVE_REQUESTS.with(|b| {
                if let Some(mut req) = b.borrow_mut().remove(&id) {
                    if let Some(w) = req.writer.take() {
                        let _ = w.abort();
                    }
                    true
                } else {
                    false
                }
            });
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
