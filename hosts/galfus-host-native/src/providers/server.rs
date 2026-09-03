use futures_util::{SinkExt, StreamExt};
use galfus_contract::builtins::std_server_provider_descriptor;
use galfus_contract::{
    CancellationOutcome, ExecutionFailure, ExecutionFailureKind, HostProvider, MessageInjector,
    ProviderDescriptor, SurfaceValue, TaskAffinity,
};
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;

use tokio::sync::{mpsc, oneshot};
use url::Url;

pub struct NativeServerProvider {
    command_tx: std::sync::mpsc::Sender<ServerCommand>,
}

struct AcceptWaiter {
    completion: Completion,
}

struct WsReceiveWaiter {
    completion: Completion,
}

struct Completion {
    injector: Arc<dyn MessageInjector>,
    thread_id: galfus_core::ThreadId,
    lease: galfus_core::RequestLease,
}

fn surface_u64(value: Option<&SurfaceValue>, name: &str) -> Result<u64, ExecutionFailure> {
    match value {
        Some(SurfaceValue::U64(value)) => Ok(*value),
        _ => Err(ExecutionFailure::new(
            ExecutionFailureKind::ProviderFailure,
            format!("expected surface u64 for {name}"),
        )),
    }
}

fn surface_u32(value: Option<&SurfaceValue>, name: &str) -> Result<u32, ExecutionFailure> {
    match value {
        Some(SurfaceValue::U32(value)) => Ok(*value),
        _ => Err(ExecutionFailure::new(
            ExecutionFailureKind::ProviderFailure,
            format!("expected surface u32 for {name}"),
        )),
    }
}

fn surface_i32(value: Option<&SurfaceValue>, name: &str) -> Result<i32, ExecutionFailure> {
    match value {
        Some(SurfaceValue::I32(value)) => Ok(*value),
        _ => Err(ExecutionFailure::new(
            ExecutionFailureKind::ProviderFailure,
            format!("expected surface i32 for {name}"),
        )),
    }
}

fn surface_bytes(value: Option<&SurfaceValue>, name: &str) -> Result<Vec<u8>, ExecutionFailure> {
    match value {
        Some(SurfaceValue::Bytes(value)) => Ok(value.clone()),
        _ => Err(ExecutionFailure::new(
            ExecutionFailureKind::ProviderFailure,
            format!("expected surface bytes for {name}"),
        )),
    }
}

fn surface_headers(
    value: Option<&SurfaceValue>,
) -> Result<Vec<(String, String)>, ExecutionFailure> {
    let Some(SurfaceValue::List(headers)) = value else {
        return Err(ExecutionFailure::new(
            ExecutionFailureKind::ProviderFailure,
            "expected surface header list",
        ));
    };
    headers
        .iter()
        .map(|header| {
            let SurfaceValue::Tuple(pair) = header else {
                return Err(ExecutionFailure::new(
                    ExecutionFailureKind::ProviderFailure,
                    "expected surface header tuple",
                ));
            };
            let name = surface_bytes(pair.first(), "header name")?;
            let value = surface_bytes(pair.get(1), "header value")?;
            Ok((
                String::from_utf8(name).map_err(|_| {
                    ExecutionFailure::new(
                        ExecutionFailureKind::ProviderFailure,
                        "invalid header name",
                    )
                })?,
                String::from_utf8(value).map_err(|_| {
                    ExecutionFailure::new(
                        ExecutionFailureKind::ProviderFailure,
                        "invalid header value",
                    )
                })?,
            ))
        })
        .collect()
}

impl Completion {
    fn inject_surface(&self, result: Result<SurfaceValue, ExecutionFailure>) {
        let _ = self
            .injector
            .inject_surface_response(self.thread_id, self.lease, result);
    }

    fn inject_bool(&self, value: bool) {
        self.inject_surface(Ok(SurfaceValue::Bool(value)));
    }
}

struct PendingRequest {
    request_id: u64,
    url: Url,
    method: String,
    headers: Vec<(String, String)>,
    body: Option<IncomingRequestBody>,
    upgrade: Option<hyper::upgrade::OnUpgrade>,
    websocket_key: Option<String>,
}

struct IncomingRequestBody {
    body: hyper::body::Incoming,
    pending: Vec<u8>,
}

enum ServerCommand {
    Bind {
        port: i32,
        completion: Completion,
    },
    Accept {
        _server_id: u64,
        completion: Completion,
    },
    Respond {
        request_id: u64,
        status: i32,
        headers: Vec<(String, String)>,
        body: Option<Vec<u8>>,
        is_upgrade: bool,
        completion: Completion,
    },
    RequestRead {
        request_id: u64,
        max_bytes: u32,
        completion: Completion,
    },
    RequestClose {
        request_id: u64,
        completion: Completion,
    },
    WsReceive {
        ws_id: u64,
        completion: Completion,
    },
    WsSend {
        ws_id: u64,
        data: Vec<u8>,
        completion: Completion,
    },
    WsClose {
        ws_id: u64,
        completion: Completion,
    },
    InternalRequestReceived {
        req: PendingRequest,
        response_tx: oneshot::Sender<(Response<Full<Bytes>>, bool)>,
    },
    InternalRequestBodyRead {
        request_id: u64,
        body: IncomingRequestBody,
        result: Result<Option<Vec<u8>>, String>,
        completion: Completion,
    },
    InternalWsUpgraded {
        ws_id: u64,
        stream: tokio_tungstenite::WebSocketStream<TokioIo<hyper::upgrade::Upgraded>>,
    },
}

impl Default for NativeServerProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl NativeServerProvider {
    pub fn new() -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        let cmd_tx_clone = tx.clone();

        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async move {
                Self::run_reactor(rx, cmd_tx_clone).await;
            });
        });

        Self { command_tx: tx }
    }

    async fn run_reactor(
        rx: std::sync::mpsc::Receiver<ServerCommand>,
        internal_tx: std::sync::mpsc::Sender<ServerCommand>,
    ) {
        let mut next_server_id = 1;
        let mut next_request_id = 1;

        let mut accept_waiters: VecDeque<AcceptWaiter> = VecDeque::new();
        let mut pending_requests: VecDeque<PendingRequest> = VecDeque::new();
        let mut request_bodies: HashMap<u64, IncomingRequestBody> = HashMap::new();

        let mut response_channels: HashMap<
            u64,
            (
                oneshot::Sender<(Response<Full<Bytes>>, bool)>,
                Option<hyper::upgrade::OnUpgrade>,
                Option<String>,
            ),
        > = HashMap::new();

        type WsStream = tokio_tungstenite::WebSocketStream<TokioIo<hyper::upgrade::Upgraded>>;
        let mut active_websockets: HashMap<u64, WsStream> = HashMap::new();
        // A Galfus handler can await its first message immediately after sending
        // the 101 response. Keep that request until Hyper finishes the upgrade.
        let mut ws_receive_waiters: HashMap<u64, WsReceiveWaiter> = HashMap::new();

        let (async_tx, mut async_rx) = mpsc::unbounded_channel::<ServerCommand>();

        std::thread::spawn(move || {
            while let Ok(cmd) = rx.recv() {
                let _ = async_tx.send(cmd);
            }
        });

        while let Some(cmd) = async_rx.recv().await {
            match cmd {
                ServerCommand::Bind { port, completion } => {
                    let addr = SocketAddr::from(([0, 0, 0, 0], port as u16));
                    match TcpListener::bind(addr).await {
                        Ok(listener) => {
                            let server_id = next_server_id;
                            next_server_id += 1;

                            let itx = internal_tx.clone();
                            tokio::spawn(async move {
                                loop {
                                    if let Ok((stream, _)) = listener.accept().await {
                                        let io = TokioIo::new(stream);
                                        let itx = itx.clone();
                                        tokio::spawn(async move {
                                            let service = service_fn(
                                                move |mut req: Request<hyper::body::Incoming>| {
                                                    let itx = itx.clone();
                                                    async move {
                                                        let websocket_key = req
                                                            .headers()
                                                            .get(hyper::header::SEC_WEBSOCKET_KEY)
                                                            .and_then(|value| value.to_str().ok())
                                                            .map(str::to_owned);
                                                        let upgrade = websocket_key
                                                            .as_ref()
                                                            .map(|_| hyper::upgrade::on(&mut req));
                                                        let url = format!(
                                                            "http://localhost{}{}",
                                                            req.uri().path(),
                                                            req.uri()
                                                                .query()
                                                                .map(|q| format!("?{}", q))
                                                                .unwrap_or_default()
                                                        );
                                                        let parsed_url = Url::parse(&url).unwrap();

                                                        let mut headers = vec![];
                                                        for (k, v) in req.headers() {
                                                            headers.push((
                                                                k.as_str().to_string(),
                                                                v.to_str()
                                                                    .unwrap_or("")
                                                                    .to_string(),
                                                            ));
                                                        }

                                                        let method_str =
                                                            req.method().as_str().to_string();

                                                        let (_, body) = req.into_parts();

                                                        let (res_tx, res_rx) = oneshot::channel();

                                                        let req_id = 0; // placeholder

                                                        let pending = PendingRequest {
                                                            request_id: req_id,
                                                            url: parsed_url,
                                                            method: method_str,
                                                            headers,
                                                            body: Some(IncomingRequestBody {
                                                                body,
                                                                pending: Vec::new(),
                                                            }),
                                                            upgrade,
                                                            websocket_key,
                                                        };

                                                        let _ = itx.send(ServerCommand::InternalRequestReceived {
                                                        req: pending,
                                                        response_tx: res_tx,
                                                    });

                                                        if let Ok((res, is_upgrade)) = res_rx.await
                                                        {
                                                            if is_upgrade {
                                                                // Logic for upgrade handled by client?
                                                                // In hyper, we return 101 and use `hyper::upgrade::on`
                                                                // We'll simulate this shortly
                                                            }
                                                            Ok::<_, hyper::Error>(res)
                                                        } else {
                                                            Ok(Response::builder()
                                                                .status(500)
                                                                .body(Full::new(Bytes::new()))
                                                                .unwrap())
                                                        }
                                                    }
                                                },
                                            );

                                            let _ = hyper_util::server::conn::auto::Builder::new(
                                                hyper_util::rt::TokioExecutor::new(),
                                            )
                                            .serve_connection_with_upgrades(io, service)
                                            .await;
                                        });
                                    }
                                }
                            });

                            completion.inject_surface(Ok(SurfaceValue::U64(server_id)));
                        }
                        Err(e) => {
                            completion.inject_surface(Err(ExecutionFailure::new(
                                ExecutionFailureKind::ProviderFailure,
                                e.to_string(),
                            )));
                        }
                    }
                }
                ServerCommand::InternalRequestReceived {
                    mut req,
                    response_tx,
                } => {
                    req.request_id = next_request_id;
                    next_request_id += 1;
                    let upgrade = req.upgrade.take();
                    let websocket_key = req.websocket_key.take();

                    let body = req
                        .body
                        .take()
                        .expect("server request body must be present");
                    request_bodies.insert(req.request_id, body);

                    response_channels.insert(req.request_id, (response_tx, upgrade, websocket_key));

                    if let Some(waiter) = accept_waiters.pop_front() {
                        Self::inject_request(waiter, req);
                    } else {
                        pending_requests.push_back(req);
                    }
                }
                ServerCommand::Accept { completion, .. } => {
                    if let Some(req) = pending_requests.pop_front() {
                        Self::inject_request(AcceptWaiter { completion }, req);
                    } else {
                        accept_waiters.push_back(AcceptWaiter { completion });
                    }
                }
                ServerCommand::Respond {
                    request_id,
                    status,
                    headers,
                    body,
                    is_upgrade,
                    completion,
                } => {
                    if let Some((tx, upgrade, websocket_key)) =
                        response_channels.remove(&request_id)
                    {
                        let mut builder = Response::builder().status(status as u16);
                        for (k, v) in headers {
                            builder = builder.header(k, v);
                        }

                        if is_upgrade {
                            let Some(websocket_key) = websocket_key else {
                                completion.inject_bool(false);
                                continue;
                            };
                            builder = builder
                                .status(101)
                                .header(hyper::header::CONNECTION, "Upgrade")
                                .header(hyper::header::UPGRADE, "websocket")
                                .header(
                                    hyper::header::SEC_WEBSOCKET_ACCEPT,
                                    tungstenite::handshake::derive_accept_key(
                                        websocket_key.as_bytes(),
                                    ),
                                );
                        }

                        let bytes = body.unwrap_or_default();
                        let response = builder.body(Full::new(Bytes::from(bytes))).unwrap();

                        let _ = tx.send((response, is_upgrade));
                        if is_upgrade && let Some(upgrade) = upgrade {
                            let itx = internal_tx.clone();
                            tokio::spawn(async move {
                                if let Ok(upgraded) = upgrade.await {
                                    let stream =
                                        tokio_tungstenite::WebSocketStream::from_raw_socket(
                                            TokioIo::new(upgraded),
                                            tokio_tungstenite::tungstenite::protocol::Role::Server,
                                            None,
                                        )
                                        .await;
                                    let _ = itx.send(ServerCommand::InternalWsUpgraded {
                                        ws_id: request_id,
                                        stream,
                                    });
                                }
                            });
                        }
                        completion.inject_bool(true);
                    } else {
                        completion.inject_bool(false);
                    }
                }
                ServerCommand::RequestRead {
                    request_id,
                    max_bytes,
                    completion,
                } => {
                    let Some(body) = request_bodies.remove(&request_id) else {
                        completion.inject_surface(Ok(SurfaceValue::Null));
                        continue;
                    };
                    let itx = internal_tx.clone();
                    tokio::spawn(async move {
                        let (body, result) =
                            Self::read_request_body(body, max_bytes as usize).await;
                        let _ = itx.send(ServerCommand::InternalRequestBodyRead {
                            request_id,
                            body,
                            result,
                            completion,
                        });
                    });
                }
                ServerCommand::RequestClose {
                    request_id,
                    completion,
                } => {
                    request_bodies.remove(&request_id);
                    completion.inject_bool(true);
                }
                ServerCommand::InternalRequestBodyRead {
                    request_id,
                    body,
                    result,
                    completion,
                } => match result {
                    Ok(Some(chunk)) => {
                        request_bodies.insert(request_id, body);
                        completion.inject_surface(Ok(SurfaceValue::Bytes(chunk)));
                    }
                    Ok(None) => completion.inject_surface(Ok(SurfaceValue::Null)),
                    Err(error) => completion.inject_surface(Err(ExecutionFailure::new(
                        ExecutionFailureKind::ProviderFailure,
                        error,
                    ))),
                },
                ServerCommand::InternalWsUpgraded { ws_id, stream } => {
                    if let Some(waiter) = ws_receive_waiters.remove(&ws_id) {
                        Self::receive_ws_message(ws_id, stream, waiter, internal_tx.clone());
                    } else {
                        active_websockets.insert(ws_id, stream);
                    }
                }
                ServerCommand::WsReceive { ws_id, completion } => {
                    if let Some(stream) = active_websockets.remove(&ws_id) {
                        Self::receive_ws_message(
                            ws_id,
                            stream,
                            WsReceiveWaiter { completion },
                            internal_tx.clone(),
                        );
                    } else {
                        ws_receive_waiters.insert(ws_id, WsReceiveWaiter { completion });
                    }
                }
                ServerCommand::WsSend {
                    ws_id,
                    data,
                    completion,
                } => {
                    if let Some(mut stream) = active_websockets.remove(&ws_id) {
                        let itx = internal_tx.clone();
                        tokio::spawn(async move {
                            let success = stream
                                .send(tokio_tungstenite::tungstenite::Message::Binary(data))
                                .await
                                .is_ok();
                            completion.inject_bool(success);
                            let _ = itx.send(ServerCommand::InternalWsUpgraded { ws_id, stream });
                        });
                    } else {
                        completion.inject_bool(false);
                    }
                }
                ServerCommand::WsClose { ws_id, completion } => {
                    active_websockets.remove(&ws_id);
                    completion.inject_bool(true);
                }
            }
        }
    }

    fn inject_request(waiter: AcceptWaiter, req: PendingRequest) {
        let href = req.url.as_str().as_bytes().to_vec();
        let protocol = req.url.scheme().as_bytes().to_vec();
        let host = req.url.host_str().unwrap_or("").as_bytes().to_vec();
        let hostname = req.url.domain().unwrap_or("").as_bytes().to_vec();
        let path = req.url.path().as_bytes().to_vec();
        let search = req.url.query().unwrap_or("").as_bytes().to_vec();
        let hash = req.url.fragment().unwrap_or("").as_bytes().to_vec();
        let origin = req.url.origin().ascii_serialization().into_bytes();

        let headers = req
            .headers
            .into_iter()
            .map(|(name, value)| {
                SurfaceValue::Tuple(vec![
                    SurfaceValue::Bytes(name.into_bytes()),
                    SurfaceValue::Bytes(value.into_bytes()),
                ])
            })
            .collect();
        waiter
            .completion
            .inject_surface(Ok(SurfaceValue::Struct(vec![
                ("id".to_string(), SurfaceValue::U64(req.request_id)),
                (
                    "url".to_string(),
                    SurfaceValue::Struct(vec![
                        ("href".to_string(), SurfaceValue::Bytes(href)),
                        ("protocol".to_string(), SurfaceValue::Bytes(protocol)),
                        ("host".to_string(), SurfaceValue::Bytes(host)),
                        ("hostname".to_string(), SurfaceValue::Bytes(hostname)),
                        ("pathname".to_string(), SurfaceValue::Bytes(path)),
                        ("search".to_string(), SurfaceValue::Bytes(search)),
                        ("hash".to_string(), SurfaceValue::Bytes(hash)),
                        ("origin".to_string(), SurfaceValue::Bytes(origin)),
                    ]),
                ),
                (
                    "method".to_string(),
                    SurfaceValue::Bytes(req.method.into_bytes()),
                ),
                ("headers".to_string(), SurfaceValue::List(headers)),
                ("body".to_string(), SurfaceValue::U64(req.request_id)),
            ])));
    }

    async fn read_request_body(
        mut body: IncomingRequestBody,
        max_bytes: usize,
    ) -> (IncomingRequestBody, Result<Option<Vec<u8>>, String>) {
        let max_bytes = max_bytes.max(1);
        if !body.pending.is_empty() {
            let split_at = body.pending.len().min(max_bytes);
            let remainder = body.pending.split_off(split_at);
            let chunk = std::mem::replace(&mut body.pending, remainder);
            return (body, Ok(Some(chunk)));
        }

        loop {
            match body.body.frame().await {
                Some(Ok(frame)) => match frame.into_data() {
                    Ok(data) if data.is_empty() => continue,
                    Ok(data) => {
                        let split_at = data.len().min(max_bytes);
                        let chunk = data[..split_at].to_vec();
                        if split_at < data.len() {
                            body.pending.extend_from_slice(&data[split_at..]);
                        }
                        return (body, Ok(Some(chunk)));
                    }
                    Err(_) => continue,
                },
                Some(Err(error)) => return (body, Err(error.to_string())),
                None => return (body, Ok(None)),
            }
        }
    }

    fn receive_ws_message(
        ws_id: u64,
        mut stream: tokio_tungstenite::WebSocketStream<TokioIo<hyper::upgrade::Upgraded>>,
        waiter: WsReceiveWaiter,
        internal_tx: std::sync::mpsc::Sender<ServerCommand>,
    ) {
        tokio::spawn(async move {
            loop {
                match stream.next().await {
                    Some(Ok(tokio_tungstenite::tungstenite::Message::Ping(_)))
                    | Some(Ok(tokio_tungstenite::tungstenite::Message::Pong(_))) => continue,
                    Some(Ok(msg)) => {
                        let (code, data, should_resume) = match msg {
                            tokio_tungstenite::tungstenite::Message::Text(text) => {
                                (1, Some(text.into_bytes()), true)
                            }
                            tokio_tungstenite::tungstenite::Message::Binary(data) => {
                                (2, Some(data), true)
                            }
                            tokio_tungstenite::tungstenite::Message::Close(close) => (
                                close.map(|frame| frame.code.into()).unwrap_or(1000) as i32,
                                None,
                                false,
                            ),
                            _ => continue,
                        };

                        waiter
                            .completion
                            .inject_surface(Ok(SurfaceValue::Struct(vec![
                                ("status".to_string(), SurfaceValue::I32(code)),
                                (
                                    "msg".to_string(),
                                    data.map(SurfaceValue::Bytes).unwrap_or(SurfaceValue::Null),
                                ),
                            ])));

                        if should_resume {
                            let _ = internal_tx
                                .send(ServerCommand::InternalWsUpgraded { ws_id, stream });
                        }
                        return;
                    }
                    Some(Err(error)) => {
                        waiter
                            .completion
                            .inject_surface(Ok(SurfaceValue::Struct(vec![
                                ("status".to_string(), SurfaceValue::I32(-1)),
                                (
                                    "msg".to_string(),
                                    SurfaceValue::Bytes(error.to_string().into_bytes()),
                                ),
                            ])));
                        return;
                    }
                    None => {
                        let message = b"WebSocket stream ended without a close frame".to_vec();
                        waiter
                            .completion
                            .inject_surface(Ok(SurfaceValue::Struct(vec![
                                ("status".to_string(), SurfaceValue::I32(-1)),
                                ("msg".to_string(), SurfaceValue::Bytes(message)),
                            ])));
                        return;
                    }
                }
            }
        });
    }
}

impl HostProvider for NativeServerProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        std_server_provider_descriptor()
    }

    fn affinity(&self, _name: &str) -> TaskAffinity {
        TaskAffinity::Main // Always fast offload to background
    }

    fn dispatch_surface(
        &mut self,
        thread_id: galfus_core::ThreadId,
        request_lease: galfus_core::RequestLease,
        name: &str,
        args: &[SurfaceValue],
        injector: Arc<dyn MessageInjector>,
    ) -> bool {
        let completion = Completion {
            injector: injector.clone(),
            thread_id,
            lease: request_lease,
        };
        let command = (|| -> Result<ServerCommand, ExecutionFailure> {
            match name {
                "server_bind" => Ok(ServerCommand::Bind {
                    port: surface_i32(args.first(), "port")?,
                    completion,
                }),
                "server_accept" => Ok(ServerCommand::Accept {
                    _server_id: surface_u64(args.first(), "server ID")?,
                    completion,
                }),
                "server_respond" => {
                    let body = match args.get(3) {
                        Some(SurfaceValue::Bytes(body)) => Some(body.clone()),
                        Some(SurfaceValue::Null) => None,
                        _ => {
                            return Err(ExecutionFailure::new(
                                ExecutionFailureKind::ProviderFailure,
                                "expected nullable surface response body",
                            ));
                        }
                    };
                    let is_upgrade = match args.get(4) {
                        Some(SurfaceValue::Bool(value)) => *value,
                        _ => {
                            return Err(ExecutionFailure::new(
                                ExecutionFailureKind::ProviderFailure,
                                "expected surface response upgrade flag",
                            ));
                        }
                    };
                    Ok(ServerCommand::Respond {
                        request_id: surface_u64(args.first(), "request ID")?,
                        status: surface_i32(args.get(1), "response status")?,
                        headers: surface_headers(args.get(2))?,
                        body,
                        is_upgrade,
                        completion,
                    })
                }
                "server_request_read" => Ok(ServerCommand::RequestRead {
                    request_id: surface_u64(args.first(), "request ID")?,
                    max_bytes: surface_u32(args.get(1), "maximum read size")?,
                    completion,
                }),
                "server_request_close" => Ok(ServerCommand::RequestClose {
                    request_id: surface_u64(args.first(), "request ID")?,
                    completion,
                }),
                "server_ws_receive" => Ok(ServerCommand::WsReceive {
                    ws_id: surface_u64(args.first(), "WebSocket ID")?,
                    completion,
                }),
                "server_ws_send" => Ok(ServerCommand::WsSend {
                    ws_id: surface_u64(args.first(), "WebSocket ID")?,
                    data: surface_bytes(args.get(1), "WebSocket data")?,
                    completion,
                }),
                "server_ws_close" => Ok(ServerCommand::WsClose {
                    ws_id: surface_u64(args.first(), "WebSocket ID")?,
                    completion,
                }),
                _ => Err(ExecutionFailure::new(
                    ExecutionFailureKind::ProviderFailure,
                    format!("function {name} is not implemented in NativeServerProvider"),
                )),
            }
        })();
        match command {
            Ok(command) => {
                let _ = self.command_tx.send(command);
            }
            Err(error) => {
                let _ = injector.inject_surface_response(thread_id, request_lease, Err(error));
            }
        }
        true
    }

    fn cancel(
        &mut self,
        _thread_id: galfus_core::ThreadId,
        _request_lease: galfus_core::RequestLease,
    ) -> CancellationOutcome {
        CancellationOutcome::BestEffort
    }
}
