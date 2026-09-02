use galfus_contract::builtins::std_http_provider_descriptor;
use galfus_contract::{
    BoundaryType, BoundaryValue, CancellationOutcome, ExecutionFailure, ExecutionFailureKind,
    HostProvider, MessageInjector, ProviderDescriptor, SurfaceValue, TaskAffinity,
};
use std::sync::Arc;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

pub struct WebHttpProvider;
impl WebHttpProvider {
    pub fn new() -> Self {
        Self
    }
}
impl Default for WebHttpProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl HostProvider for WebHttpProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        std_http_provider_descriptor()
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
        if name != "http_request" {
            let _ = injector.inject_system_response(
                thread_id,
                request_lease,
                Err(ExecutionFailure::new(
                    ExecutionFailureKind::ProviderFailure,
                    format!("function {name} is not implemented in WebHttpProvider"),
                )),
            );
            return;
        }
        let (
            Some(BoundaryValue::Bytes(method)),
            Some(BoundaryValue::Bytes(url)),
            Some(BoundaryValue::Array {
                values: headers, ..
            }),
            body,
        ) = (args.first(), args.get(1), args.get(2), args.get(3))
        else {
            let _ = injector.inject_system_response(
                thread_id,
                request_lease,
                Err(ExecutionFailure::new(
                    ExecutionFailureKind::ProviderFailure,
                    "invalid HTTP request arguments".to_string(),
                )),
            );
            return;
        };
        let (Ok(method), Ok(url)) = (
            String::from_utf8(method.clone()),
            String::from_utf8(url.clone()),
        ) else {
            let _ = injector.inject_system_response(
                thread_id,
                request_lease,
                Err(ExecutionFailure::new(
                    ExecutionFailureKind::ProviderFailure,
                    "HTTP method and URL must be UTF-8".to_string(),
                )),
            );
            return;
        };
        let init = web_sys::RequestInit::new();
        init.set_method(&method);
        init.set_mode(web_sys::RequestMode::Cors);
        let request_headers = web_sys::Headers::new().unwrap();
        for header in headers {
            if let BoundaryValue::Tuple(pair) = header
                && let [BoundaryValue::Bytes(key), BoundaryValue::Bytes(value)] = pair.as_slice()
                && let (Ok(key), Ok(value)) = (std::str::from_utf8(key), std::str::from_utf8(value))
            {
                let _ = request_headers.append(key, value);
            }
        }
        init.set_headers(&request_headers);
        if let Some(BoundaryValue::Bytes(body)) = body {
            init.set_body(&js_sys::Uint8Array::from(body.as_slice()).into());
        }
        let Ok(request) = web_sys::Request::new_with_str_and_init(&url, &init) else {
            let _ = injector.inject_system_response(
                thread_id,
                request_lease,
                Err(ExecutionFailure::new(
                    ExecutionFailureKind::ProviderFailure,
                    "failed to construct HTTP request".to_string(),
                )),
            );
            return;
        };
        wasm_bindgen_futures::spawn_local(async move {
            let result = async {
                let fetch = js_sys::Reflect::get(
                    &js_sys::global(),
                    &wasm_bindgen::JsValue::from_str("fetch"),
                )
                .map_err(|_| ())?
                .dyn_into::<js_sys::Function>()
                .map_err(|_| ())?;
                let response = JsFuture::from(js_sys::Promise::from(
                    fetch.call1(&js_sys::global(), &request).map_err(|_| ())?,
                ))
                .await
                .map_err(|_| ())?
                .dyn_into::<web_sys::Response>()
                .map_err(|_| ())?;
                let body = JsFuture::from(response.array_buffer().map_err(|_| ())?)
                    .await
                    .map_err(|_| ())?;
                Ok::<_, ()>(BoundaryValue::Tuple(vec![
                    BoundaryValue::I32(response.status() as i32),
                    BoundaryValue::Array {
                        element_type: BoundaryType::Tuple(vec![
                            BoundaryType::Array(Box::new(BoundaryType::U8)),
                            BoundaryType::Array(Box::new(BoundaryType::U8)),
                        ]),
                        values: Vec::new(),
                    },
                    BoundaryValue::Bytes(js_sys::Uint8Array::new(&body).to_vec()),
                ]))
            }
            .await
            .map_err(|_| {
                ExecutionFailure::new(
                    ExecutionFailureKind::ProviderFailure,
                    "HTTP request failed".to_string(),
                )
            });
            let _ = injector.inject_system_response(thread_id, request_lease, result);
        });
    }

    fn dispatch_surface(
        &mut self,
        thread_id: galfus_core::ThreadId,
        request_lease: galfus_core::RequestLease,
        name: &str,
        args: &[SurfaceValue],
        injector: Arc<dyn MessageInjector>,
    ) -> bool {
        if name != "http_request" || args.len() != 4 {
            return false;
        }
        let (
            SurfaceValue::Bytes(method),
            SurfaceValue::Bytes(url),
            SurfaceValue::List(headers),
            body,
        ) = (&args[0], &args[1], &args[2], &args[3])
        else {
            return false;
        };
        let (Ok(method), Ok(url)) = (
            String::from_utf8(method.clone()),
            String::from_utf8(url.clone()),
        ) else {
            return false;
        };
        let init = web_sys::RequestInit::new();
        init.set_method(&method);
        init.set_mode(web_sys::RequestMode::Cors);
        let request_headers = web_sys::Headers::new().unwrap();
        for header in headers {
            let SurfaceValue::Struct(fields) = header else {
                return false;
            };
            let name = fields
                .iter()
                .find_map(|(name, value)| (name == "name").then_some(value));
            let value = fields
                .iter()
                .find_map(|(name, value)| (name == "value").then_some(value));
            let (Some(SurfaceValue::Bytes(name)), Some(SurfaceValue::Bytes(value))) = (name, value)
            else {
                return false;
            };
            let (Ok(name), Ok(value)) = (std::str::from_utf8(name), std::str::from_utf8(value))
            else {
                return false;
            };
            let _ = request_headers.append(name, value);
        }
        init.set_headers(&request_headers);
        match body {
            SurfaceValue::Bytes(body) => {
                init.set_body(&js_sys::Uint8Array::from(body.as_slice()).into())
            }
            SurfaceValue::Null => {}
            _ => return false,
        }
        let Ok(request) = web_sys::Request::new_with_str_and_init(&url, &init) else {
            return false;
        };
        wasm_bindgen_futures::spawn_local(async move {
            let result = async {
                let fetch = js_sys::Reflect::get(
                    &js_sys::global(),
                    &wasm_bindgen::JsValue::from_str("fetch"),
                )
                .map_err(|_| ())?
                .dyn_into::<js_sys::Function>()
                .map_err(|_| ())?;
                let response = JsFuture::from(js_sys::Promise::from(
                    fetch.call1(&js_sys::global(), &request).map_err(|_| ())?,
                ))
                .await
                .map_err(|_| ())?
                .dyn_into::<web_sys::Response>()
                .map_err(|_| ())?;
                let status = response.status() as i32;
                let body = JsFuture::from(response.array_buffer().map_err(|_| ())?)
                    .await
                    .map_err(|_| ())?;
                Ok::<_, ()>(SurfaceValue::Struct(vec![
                    ("status".to_string(), SurfaceValue::I32(status)),
                    ("headers".to_string(), SurfaceValue::List(Vec::new())),
                    (
                        "body".to_string(),
                        SurfaceValue::Bytes(js_sys::Uint8Array::new(&body).to_vec()),
                    ),
                ]))
            }
            .await
            .map_err(|_| {
                ExecutionFailure::new(ExecutionFailureKind::ProviderFailure, "HTTP request failed")
            });
            let _ = injector.inject_surface_response(thread_id, request_lease, result);
        });
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
