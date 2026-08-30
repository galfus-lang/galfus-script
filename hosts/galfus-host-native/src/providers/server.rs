use futures_util::{SinkExt, StreamExt};
use galfus_contract::builtins::std_server_provider_descriptor;
use galfus_contract::{
    BoundaryType, BoundaryValue, CancellationOutcome, ExecutionFailure, ExecutionFailureKind,
    HostProvider, MessageInjector, ProviderDescriptor, TaskAffinity,
};
use http_body_util::Full;
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
    injector: Arc<dyn MessageInjector>,
    thread_id: galfus_core::ThreadId,
    lease: galfus_core::RequestLease,
}

struct WsReceiveWaiter {
    injector: Arc<dyn MessageInjector>,
    thread_id: galfus_core::ThreadId,
    lease: galfus_core::RequestLease,
}

struct PendingRequest {
    request_id: u64,
    url: Url,
    method: String,
    headers: Vec<(String, String)>,
    body: Option<Vec<u8>>,
    upgrade: Option<hyper::upgrade::OnUpgrade>,
    websocket_key: Option<String>,
}

enum ServerCommand {
    Bind {
        port: i32,
        injector: Arc<dyn MessageInjector>,
        thread_id: galfus_core::ThreadId,
        lease: galfus_core::RequestLease,
    },
    Accept {
        _server_id: u64,
        injector: Arc<dyn MessageInjector>,
        thread_id: galfus_core::ThreadId,
        lease: galfus_core::RequestLease,
    },
    Respond {
        request_id: u64,
        status: i32,
        headers: Vec<(String, String)>,
        body: Option<Vec<u8>>,
        is_upgrade: bool,
        injector: Arc<dyn MessageInjector>,
        thread_id: galfus_core::ThreadId,
        lease: galfus_core::RequestLease,
    },
    WsReceive {
        ws_id: u64,
        injector: Arc<dyn MessageInjector>,
        thread_id: galfus_core::ThreadId,
        lease: galfus_core::RequestLease,
    },
    WsSend {
        ws_id: u64,
        data: Vec<u8>,
        injector: Arc<dyn MessageInjector>,
        thread_id: galfus_core::ThreadId,
        lease: galfus_core::RequestLease,
    },
    WsClose {
        ws_id: u64,
        injector: Arc<dyn MessageInjector>,
        thread_id: galfus_core::ThreadId,
        lease: galfus_core::RequestLease,
    },
    InternalRequestReceived {
        req: PendingRequest,
        response_tx: oneshot::Sender<(Response<Full<Bytes>>, bool)>,
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
                ServerCommand::Bind {
                    port,
                    injector,
                    thread_id,
                    lease,
                } => {
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
                                                        use http_body_util::BodyExt;
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

                                                        let body_bytes = req
                                                            .collect()
                                                            .await
                                                            .map(|b| b.to_bytes().to_vec())
                                                            .ok();

                                                        let (res_tx, res_rx) = oneshot::channel();

                                                        let req_id = 0; // placeholder

                                                        let pending = PendingRequest {
                                                            request_id: req_id,
                                                            url: parsed_url,
                                                            method: method_str,
                                                            headers,
                                                            body: body_bytes,
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

                            let _ = injector.inject_system_response(
                                thread_id,
                                lease,
                                Ok(BoundaryValue::U64(server_id)),
                            );
                        }
                        Err(e) => {
                            let _ = injector.inject_system_response(
                                thread_id,
                                lease,
                                Err(ExecutionFailure::new(
                                    ExecutionFailureKind::ProviderFailure,
                                    e.to_string(),
                                )),
                            );
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

                    response_channels.insert(req.request_id, (response_tx, upgrade, websocket_key));

                    if let Some(waiter) = accept_waiters.pop_front() {
                        Self::inject_request(waiter, req);
                    } else {
                        pending_requests.push_back(req);
                    }
                }
                ServerCommand::Accept {
                    injector,
                    thread_id,
                    lease,
                    ..
                } => {
                    if let Some(req) = pending_requests.pop_front() {
                        Self::inject_request(
                            AcceptWaiter {
                                injector,
                                thread_id,
                                lease,
                            },
                            req,
                        );
                    } else {
                        accept_waiters.push_back(AcceptWaiter {
                            injector,
                            thread_id,
                            lease,
                        });
                    }
                }
                ServerCommand::Respond {
                    request_id,
                    status,
                    headers,
                    body,
                    is_upgrade,
                    injector,
                    thread_id,
                    lease,
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
                                let _ = injector.inject_system_response(
                                    thread_id,
                                    lease,
                                    Ok(BoundaryValue::Bool(false)),
                                );
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
                        let _ = injector.inject_system_response(
                            thread_id,
                            lease,
                            Ok(BoundaryValue::Bool(true)),
                        );
                    } else {
                        let _ = injector.inject_system_response(
                            thread_id,
                            lease,
                            Ok(BoundaryValue::Bool(false)),
                        );
                    }
                }
                ServerCommand::InternalWsUpgraded { ws_id, stream } => {
                    if let Some(waiter) = ws_receive_waiters.remove(&ws_id) {
                        Self::receive_ws_message(ws_id, stream, waiter, internal_tx.clone());
                    } else {
                        active_websockets.insert(ws_id, stream);
                    }
                }
                ServerCommand::WsReceive {
                    ws_id,
                    injector,
                    thread_id,
                    lease,
                } => {
                    if let Some(stream) = active_websockets.remove(&ws_id) {
                        Self::receive_ws_message(
                            ws_id,
                            stream,
                            WsReceiveWaiter {
                                injector,
                                thread_id,
                                lease,
                            },
                            internal_tx.clone(),
                        );
                    } else {
                        ws_receive_waiters.insert(
                            ws_id,
                            WsReceiveWaiter {
                                injector,
                                thread_id,
                                lease,
                            },
                        );
                    }
                }
                ServerCommand::WsSend {
                    ws_id,
                    data,
                    injector,
                    thread_id,
                    lease,
                } => {
                    if let Some(mut stream) = active_websockets.remove(&ws_id) {
                        let itx = internal_tx.clone();
                        tokio::spawn(async move {
                            let success = stream
                                .send(tokio_tungstenite::tungstenite::Message::Binary(data))
                                .await
                                .is_ok();
                            let _ = injector.inject_system_response(
                                thread_id,
                                lease,
                                Ok(BoundaryValue::Bool(success)),
                            );
                            let _ = itx.send(ServerCommand::InternalWsUpgraded { ws_id, stream });
                        });
                    } else {
                        let _ = injector.inject_system_response(
                            thread_id,
                            lease,
                            Ok(BoundaryValue::Bool(false)),
                        );
                    }
                }
                ServerCommand::WsClose {
                    ws_id,
                    injector,
                    thread_id,
                    lease,
                } => {
                    active_websockets.remove(&ws_id);
                    let _ = injector.inject_system_response(
                        thread_id,
                        lease,
                        Ok(BoundaryValue::Bool(true)),
                    );
                }
            }
        }
    }

    fn inject_request(waiter: AcceptWaiter, req: PendingRequest) {
        let href = req.url.as_str().to_string().into_bytes();
        let protocol = req.url.scheme().to_string().into_bytes();
        let host = req.url.host_str().unwrap_or("").to_string().into_bytes();
        let hostname = req.url.domain().unwrap_or("").to_string().into_bytes();
        let path = req.url.path().to_string().into_bytes();
        let search = req.url.query().unwrap_or("").to_string().into_bytes();
        let hash = req.url.fragment().unwrap_or("").to_string().into_bytes();
        let origin = req.url.origin().ascii_serialization().into_bytes();

        let url_tuple = BoundaryValue::Tuple(vec![
            BoundaryValue::Bytes(href),
            BoundaryValue::Bytes(protocol),
            BoundaryValue::Bytes(host),
            BoundaryValue::Bytes(hostname),
            BoundaryValue::Bytes(path),
            BoundaryValue::Bytes(search),
            BoundaryValue::Bytes(hash),
            BoundaryValue::Bytes(origin),
        ]);

        let method = BoundaryValue::Bytes(req.method.into_bytes());
        let headers = BoundaryValue::Array {
            element_type: BoundaryType::Tuple(vec![
                BoundaryType::Array(Box::new(BoundaryType::U8)),
                BoundaryType::Array(Box::new(BoundaryType::U8)),
            ]),
            values: req
                .headers
                .into_iter()
                .map(|(k, v)| {
                    BoundaryValue::Tuple(vec![
                        BoundaryValue::Bytes(k.into_bytes()),
                        BoundaryValue::Bytes(v.into_bytes()),
                    ])
                })
                .collect(),
        };

        let body = req
            .body
            .map(BoundaryValue::Bytes)
            .unwrap_or(BoundaryValue::Null);

        let request_tuple = BoundaryValue::Tuple(vec![
            BoundaryValue::U64(req.request_id),
            url_tuple,
            method,
            headers,
            body,
        ]);

        let _ = waiter.injector.inject_system_response(
            waiter.thread_id,
            waiter.lease,
            Ok(request_tuple),
        );
    }

    fn receive_ws_message(
        ws_id: u64,
        mut stream: tokio_tungstenite::WebSocketStream<TokioIo<hyper::upgrade::Upgraded>>,
        waiter: WsReceiveWaiter,
        internal_tx: std::sync::mpsc::Sender<ServerCommand>,
    ) {
        tokio::spawn(async move {
            match stream.next().await {
                Some(Ok(msg)) => {
                    let (code, data) = match msg {
                        tokio_tungstenite::tungstenite::Message::Text(text) => {
                            (1, Some(text.into_bytes()))
                        }
                        tokio_tungstenite::tungstenite::Message::Binary(data) => (2, Some(data)),
                        tokio_tungstenite::tungstenite::Message::Close(close) => (
                            close.map(|frame| frame.code.into()).unwrap_or(1000) as i32,
                            None,
                        ),
                        // Ping/Pong are transport frames. Preserve the stream and
                        // let the Galfus loop await the next application message.
                        _ => (0, Some(vec![])),
                    };

                    let mut values = vec![BoundaryValue::I32(code)];
                    values.push(
                        data.map(BoundaryValue::Bytes)
                            .unwrap_or(BoundaryValue::Null),
                    );
                    let _ = waiter.injector.inject_system_response(
                        waiter.thread_id,
                        waiter.lease,
                        Ok(BoundaryValue::Tuple(values)),
                    );

                    if code != 1000 && code != 1001 {
                        let _ =
                            internal_tx.send(ServerCommand::InternalWsUpgraded { ws_id, stream });
                    }
                }
                _ => {
                    let _ = waiter.injector.inject_system_response(
                        waiter.thread_id,
                        waiter.lease,
                        Ok(BoundaryValue::Null),
                    );
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

    fn dispatch(
        &mut self,
        thread_id: galfus_core::ThreadId,
        request_lease: galfus_core::RequestLease,
        name: &str,
        args: &[BoundaryValue],
        injector: Arc<dyn MessageInjector>,
    ) {
        let cmd = match name {
            "server_bind" => {
                if let Some(BoundaryValue::I32(port)) = args.first() {
                    ServerCommand::Bind {
                        port: *port,
                        injector,
                        thread_id,
                        lease: request_lease,
                    }
                } else {
                    return;
                }
            }
            "server_accept" => {
                if let Some(BoundaryValue::U64(id)) = args.first() {
                    ServerCommand::Accept {
                        _server_id: *id,
                        injector,
                        thread_id,
                        lease: request_lease,
                    }
                } else {
                    return;
                }
            }
            "server_respond" => {
                let request_id = match args.first() {
                    Some(BoundaryValue::U64(id)) => *id,
                    _ => return,
                };
                let status = match args.get(1) {
                    Some(BoundaryValue::I32(s)) => *s,
                    _ => return,
                };

                let mut headers = vec![];
                if let Some(BoundaryValue::Array { values, .. }) = args.get(2) {
                    for v in values {
                        #[allow(clippy::collapsible_if)]
                        if let BoundaryValue::Tuple(pair) = v {
                            if let (Some(BoundaryValue::Bytes(k)), Some(BoundaryValue::Bytes(v))) =
                                (pair.first(), pair.get(1))
                            {
                                headers.push((
                                    String::from_utf8_lossy(k).into_owned(),
                                    String::from_utf8_lossy(v).into_owned(),
                                ));
                            }
                        }
                    }
                }

                let body = match args.get(3) {
                    Some(BoundaryValue::Bytes(b)) => Some(b.clone()),
                    _ => None,
                };

                let is_upgrade = match args.get(4) {
                    Some(BoundaryValue::Bool(b)) => *b,
                    _ => false,
                };

                ServerCommand::Respond {
                    request_id,
                    status,
                    headers,
                    body,
                    is_upgrade,
                    injector,
                    thread_id,
                    lease: request_lease,
                }
            }
            "server_ws_receive" => {
                if let Some(BoundaryValue::U64(id)) = args.first() {
                    ServerCommand::WsReceive {
                        ws_id: *id,
                        injector,
                        thread_id,
                        lease: request_lease,
                    }
                } else {
                    return;
                }
            }
            "server_ws_send" => {
                let ws_id = match args.first() {
                    Some(BoundaryValue::U64(id)) => *id,
                    _ => return,
                };
                let data = match args.get(1) {
                    Some(BoundaryValue::Bytes(d)) => d.clone(),
                    _ => return,
                };
                ServerCommand::WsSend {
                    ws_id,
                    data,
                    injector,
                    thread_id,
                    lease: request_lease,
                }
            }
            "server_ws_close" => {
                if let Some(BoundaryValue::U64(id)) = args.first() {
                    ServerCommand::WsClose {
                        ws_id: *id,
                        injector,
                        thread_id,
                        lease: request_lease,
                    }
                } else {
                    return;
                }
            }
            _ => {
                let _ = injector.inject_system_response(
                    thread_id,
                    request_lease,
                    Err(ExecutionFailure::new(
                        ExecutionFailureKind::ProviderFailure,
                        format!("function {name} is not implemented in NativeServerProvider"),
                    )),
                );
                return;
            }
        };

        let _ = self.command_tx.send(cmd);
    }

    fn cancel(
        &mut self,
        _thread_id: galfus_core::ThreadId,
        _request_lease: galfus_core::RequestLease,
    ) -> CancellationOutcome {
        CancellationOutcome::BestEffort
    }
}
