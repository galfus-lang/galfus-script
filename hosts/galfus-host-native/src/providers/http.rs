use galfus_contract::builtins::std_http_provider_descriptor;
use galfus_contract::{
    BoundaryType, BoundaryValue, CancellationOutcome, ExecutionFailure, ExecutionFailureKind,
    HostProvider, MessageInjector, ProviderDescriptor, SurfaceValue, TaskAffinity,
};
use std::io::Read;
use std::sync::Arc;

pub struct NativeHttpProvider;

impl NativeHttpProvider {
    pub fn new() -> Self {
        Self
    }
}

fn surface_bytes(value: &SurfaceValue, name: &str) -> Result<Vec<u8>, ExecutionFailure> {
    match value {
        SurfaceValue::Bytes(value) => Ok(value.clone()),
        _ => Err(ExecutionFailure::new(
            ExecutionFailureKind::ProviderFailure,
            format!("expected surface bytes for {name}"),
        )),
    }
}

fn surface_headers(value: &SurfaceValue) -> Result<Vec<(String, String)>, ExecutionFailure> {
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
            let name = fields
                .iter()
                .find_map(|(name, value)| (name == "name").then_some(value))
                .ok_or_else(|| {
                    ExecutionFailure::new(
                        ExecutionFailureKind::ProviderFailure,
                        "missing header name",
                    )
                })?;
            let value = fields
                .iter()
                .find_map(|(name, value)| (name == "value").then_some(value))
                .ok_or_else(|| {
                    ExecutionFailure::new(
                        ExecutionFailureKind::ProviderFailure,
                        "missing header value",
                    )
                })?;
            Ok((
                String::from_utf8(surface_bytes(name, "header name")?).map_err(|_| {
                    ExecutionFailure::new(
                        ExecutionFailureKind::ProviderFailure,
                        "invalid header name",
                    )
                })?,
                String::from_utf8(surface_bytes(value, "header value")?).map_err(|_| {
                    ExecutionFailure::new(
                        ExecutionFailureKind::ProviderFailure,
                        "invalid header value",
                    )
                })?,
            ))
        })
        .collect()
}
impl Default for NativeHttpProvider {
    fn default() -> Self {
        Self::new()
    }
}

fn bytes(args: &[BoundaryValue], index: usize, name: &str) -> Result<Vec<u8>, ExecutionFailure> {
    match args.get(index) {
        Some(BoundaryValue::Bytes(value)) => Ok(value.clone()),
        _ => Err(ExecutionFailure::new(
            ExecutionFailureKind::ProviderFailure,
            format!("expected bytes for {name}"),
        )),
    }
}

fn headers(value: Option<&BoundaryValue>) -> Result<Vec<(String, String)>, ExecutionFailure> {
    let Some(BoundaryValue::Array { values, .. }) = value else {
        return Err(ExecutionFailure::new(
            ExecutionFailureKind::ProviderFailure,
            "expected header array".to_string(),
        ));
    };
    values
        .iter()
        .map(|value| {
            let BoundaryValue::Tuple(pair) = value else {
                return Err(ExecutionFailure::new(
                    ExecutionFailureKind::ProviderFailure,
                    "expected header tuple".to_string(),
                ));
            };
            if pair.len() != 2 {
                return Err(ExecutionFailure::new(
                    ExecutionFailureKind::ProviderFailure,
                    "expected two header values".to_string(),
                ));
            }
            let name = match &pair[0] {
                BoundaryValue::Bytes(value) => String::from_utf8(value.clone()).map_err(|_| {
                    ExecutionFailure::new(
                        ExecutionFailureKind::ProviderFailure,
                        "invalid header name".to_string(),
                    )
                })?,
                _ => {
                    return Err(ExecutionFailure::new(
                        ExecutionFailureKind::ProviderFailure,
                        "expected header name bytes".to_string(),
                    ));
                }
            };
            let value = match &pair[1] {
                BoundaryValue::Bytes(value) => String::from_utf8(value.clone()).map_err(|_| {
                    ExecutionFailure::new(
                        ExecutionFailureKind::ProviderFailure,
                        "invalid header value".to_string(),
                    )
                })?,
                _ => {
                    return Err(ExecutionFailure::new(
                        ExecutionFailureKind::ProviderFailure,
                        "expected header value bytes".to_string(),
                    ));
                }
            };
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
    fn dispatch(
        &mut self,
        thread_id: galfus_core::ThreadId,
        request_lease: galfus_core::RequestLease,
        name: &str,
        args: &[BoundaryValue],
        injector: Arc<dyn MessageInjector>,
    ) {
        let result = (|| -> Result<BoundaryValue, ExecutionFailure> {
            if name != "http_request" {
                return Err(ExecutionFailure::new(
                    ExecutionFailureKind::ProviderFailure,
                    format!("function {name} is not implemented in NativeHttpProvider"),
                ));
            }
            let method = String::from_utf8(bytes(args, 0, "method")?).map_err(|_| {
                ExecutionFailure::new(
                    ExecutionFailureKind::ProviderFailure,
                    "invalid UTF-8 method".to_string(),
                )
            })?;
            let url = String::from_utf8(bytes(args, 1, "url")?).map_err(|_| {
                ExecutionFailure::new(
                    ExecutionFailureKind::ProviderFailure,
                    "invalid UTF-8 url".to_string(),
                )
            })?;
            let request = headers(args.get(2))?
                .into_iter()
                .fold(ureq::request(&method, &url), |request, (name, value)| {
                    request.set(&name, &value)
                });
            let response = match args.get(3) {
                Some(BoundaryValue::Bytes(body)) => request.send_bytes(body),
                Some(BoundaryValue::Null) => request.call(),
                _ => {
                    return Err(ExecutionFailure::new(
                        ExecutionFailureKind::ProviderFailure,
                        "expected nullable body bytes".to_string(),
                    ));
                }
            }
            .map_err(|_| {
                ExecutionFailure::new(
                    ExecutionFailureKind::ProviderFailure,
                    "HTTP request failed".to_string(),
                )
            })?;
            let status = response.status() as i32;
            let response_headers = response
                .headers_names()
                .into_iter()
                .filter_map(|name| {
                    response.header(&name).map(|value| {
                        BoundaryValue::Tuple(vec![
                            BoundaryValue::Bytes(name.into_bytes()),
                            BoundaryValue::Bytes(value.as_bytes().to_vec()),
                        ])
                    })
                })
                .collect();
            let mut body = Vec::new();
            response.into_reader().read_to_end(&mut body).map_err(|_| {
                ExecutionFailure::new(
                    ExecutionFailureKind::ProviderFailure,
                    "failed to read HTTP response".to_string(),
                )
            })?;
            Ok(BoundaryValue::Tuple(vec![
                BoundaryValue::I32(status),
                BoundaryValue::Array {
                    element_type: BoundaryType::Tuple(vec![
                        BoundaryType::Array(Box::new(BoundaryType::U8)),
                        BoundaryType::Array(Box::new(BoundaryType::U8)),
                    ]),
                    values: response_headers,
                },
                BoundaryValue::Bytes(body),
            ]))
        })();
        let _ = injector.inject_system_response(thread_id, request_lease, result);
    }

    fn dispatch_surface(
        &mut self,
        thread_id: galfus_core::ThreadId,
        request_lease: galfus_core::RequestLease,
        name: &str,
        args: &[SurfaceValue],
        injector: Arc<dyn MessageInjector>,
    ) -> bool {
        let result = (|| -> Result<SurfaceValue, ExecutionFailure> {
            if name != "http_request" || args.len() != 4 {
                return Err(ExecutionFailure::new(
                    ExecutionFailureKind::ProviderFailure,
                    "invalid surface HTTP request",
                ));
            }
            let method = String::from_utf8(surface_bytes(&args[0], "method")?).map_err(|_| {
                ExecutionFailure::new(
                    ExecutionFailureKind::ProviderFailure,
                    "invalid UTF-8 method",
                )
            })?;
            let url = String::from_utf8(surface_bytes(&args[1], "url")?).map_err(|_| {
                ExecutionFailure::new(ExecutionFailureKind::ProviderFailure, "invalid UTF-8 url")
            })?;
            let request = surface_headers(&args[2])?
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
            let mut body = Vec::new();
            response.into_reader().read_to_end(&mut body).map_err(|_| {
                ExecutionFailure::new(
                    ExecutionFailureKind::ProviderFailure,
                    "failed to read HTTP response",
                )
            })?;
            Ok(SurfaceValue::Struct(vec![
                ("status".to_string(), SurfaceValue::I32(status)),
                ("headers".to_string(), SurfaceValue::List(headers)),
                ("body".to_string(), SurfaceValue::Bytes(body)),
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
