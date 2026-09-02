use galfus_contract::{BoundaryValue, HostProvider, MessageInjectionError, MessageInjector};
use galfus_host_native::providers::{
    http::NativeHttpProvider, net::NativeNetProvider, websocket::NativeWebSocketProvider,
};
use std::io::{Read, Write};
use std::net::{TcpListener, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct Injector(Mutex<Vec<BoundaryValue>>);

impl MessageInjector for Injector {
    fn inject_system_response(
        &self,
        _thread: galfus_core::ThreadId,
        _lease: galfus_core::RequestLease,
        result: Result<BoundaryValue, galfus_contract::ExecutionFailure>,
    ) -> Result<(), MessageInjectionError> {
        self.0.lock().unwrap().push(result.unwrap());
        Ok(())
    }

    fn inject_surface_response(
        &self,
        _thread: galfus_core::ThreadId,
        _lease: galfus_core::RequestLease,
        result: Result<galfus_contract::SurfaceValue, galfus_contract::ExecutionFailure>,
    ) -> Result<(), MessageInjectionError> {
        self.0
            .lock()
            .unwrap()
            .push(surface_to_boundary(result.unwrap()));
        Ok(())
    }
}

fn boundary_to_surface(value: BoundaryValue) -> galfus_contract::SurfaceValue {
    match value {
        BoundaryValue::Null => galfus_contract::SurfaceValue::Null,
        BoundaryValue::Bool(value) => galfus_contract::SurfaceValue::Bool(value),
        BoundaryValue::I32(value) => galfus_contract::SurfaceValue::I32(value),
        BoundaryValue::U16(value) => galfus_contract::SurfaceValue::U16(value),
        BoundaryValue::U32(value) => galfus_contract::SurfaceValue::U32(value),
        BoundaryValue::U64(value) => galfus_contract::SurfaceValue::U64(value),
        BoundaryValue::Bytes(value) => galfus_contract::SurfaceValue::Bytes(value),
        BoundaryValue::Tuple(values) => galfus_contract::SurfaceValue::Tuple(
            values.into_iter().map(boundary_to_surface).collect(),
        ),
        BoundaryValue::Array { values, .. } => galfus_contract::SurfaceValue::List(
            values.into_iter().map(boundary_to_surface).collect(),
        ),
        value => panic!("unsupported test boundary value {value:?}"),
    }
}

fn surface_to_boundary(value: galfus_contract::SurfaceValue) -> BoundaryValue {
    match value {
        galfus_contract::SurfaceValue::Null => BoundaryValue::Null,
        galfus_contract::SurfaceValue::Bool(value) => BoundaryValue::Bool(value),
        galfus_contract::SurfaceValue::I32(value) => BoundaryValue::I32(value),
        galfus_contract::SurfaceValue::U16(value) => BoundaryValue::U16(value),
        galfus_contract::SurfaceValue::U32(value) => BoundaryValue::U32(value),
        galfus_contract::SurfaceValue::U64(value) => BoundaryValue::U64(value),
        galfus_contract::SurfaceValue::Bytes(value) => BoundaryValue::Bytes(value),
        galfus_contract::SurfaceValue::Tuple(values) => {
            BoundaryValue::Tuple(values.into_iter().map(surface_to_boundary).collect())
        }
        galfus_contract::SurfaceValue::List(values) => BoundaryValue::Array {
            element_type: galfus_contract::BoundaryType::Null,
            values: values.into_iter().map(surface_to_boundary).collect(),
        },
        galfus_contract::SurfaceValue::Struct(values) => BoundaryValue::Tuple(
            values
                .into_iter()
                .map(|(_, value)| surface_to_boundary(value))
                .collect(),
        ),
        value => panic!("unsupported test surface value {value:?}"),
    }
}

fn dispatch<P: HostProvider>(
    provider: &mut P,
    name: &str,
    args: Vec<BoundaryValue>,
) -> BoundaryValue {
    let injector = Arc::new(Injector(Mutex::new(Vec::new())));
    let mut args = args
        .into_iter()
        .map(boundary_to_surface)
        .collect::<Vec<_>>();
    if name == "http_request"
        && let Some(galfus_contract::SurfaceValue::List(headers)) = args.get_mut(2)
    {
        for header in headers {
            let galfus_contract::SurfaceValue::Tuple(pair) = header else {
                continue;
            };
            if pair.len() != 2 {
                continue;
            }
            *header = galfus_contract::SurfaceValue::Struct(vec![
                ("name".to_string(), pair[0].clone()),
                ("value".to_string(), pair[1].clone()),
            ]);
        }
    }
    assert!(provider.dispatch_surface(
        galfus_core::ThreadId::new(1),
        galfus_core::RequestLease::new(galfus_core::RequestId::new(1), 0),
        name,
        &args,
        injector.clone(),
    ));
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(value) = injector.0.lock().unwrap().pop() {
            return value;
        }
        assert!(Instant::now() < deadline, "provider did not complete");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn http_provider_returns_loopback_response() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 512];
        let size = stream.read(&mut request).unwrap();
        assert!(
            std::str::from_utf8(&request[..size])
                .unwrap()
                .starts_with("POST /echo HTTP/1.1")
        );
        stream
            .write_all(b"HTTP/1.1 201 Created\r\nX-Test: yes\r\nContent-Length: 4\r\n\r\npong")
            .unwrap();
    });

    let mut provider = NativeHttpProvider::new();
    let response = dispatch(
        &mut provider,
        "http_request",
        vec![
            BoundaryValue::Bytes(b"POST".to_vec()),
            BoundaryValue::Bytes(format!("http://127.0.0.1:{port}/echo").into_bytes()),
            BoundaryValue::Array {
                element_type: galfus_contract::BoundaryType::Tuple(vec![
                    galfus_contract::BoundaryType::Array(Box::new(
                        galfus_contract::BoundaryType::U8,
                    )),
                    galfus_contract::BoundaryType::Array(Box::new(
                        galfus_contract::BoundaryType::U8,
                    )),
                ]),
                values: vec![BoundaryValue::Tuple(vec![
                    BoundaryValue::Bytes(b"X-Request".to_vec()),
                    BoundaryValue::Bytes(b"value".to_vec()),
                ])],
            },
            BoundaryValue::Bytes(b"ping".to_vec()),
        ],
    );
    let body = match response {
        BoundaryValue::Tuple(values) if values[0] == BoundaryValue::I32(201) => match &values[2] {
            BoundaryValue::U64(body) => *body,
            value => panic!("unexpected HTTP body handle {value:?}"),
        },
        value => panic!("unexpected HTTP response {value:?}"),
    };
    assert_eq!(
        dispatch(
            &mut provider,
            "http_response_read",
            vec![BoundaryValue::U64(body), BoundaryValue::U32(2)],
        ),
        BoundaryValue::Bytes(b"po".to_vec())
    );
    assert_eq!(
        dispatch(
            &mut provider,
            "http_response_read",
            vec![BoundaryValue::U64(body), BoundaryValue::U32(2)],
        ),
        BoundaryValue::Bytes(b"ng".to_vec())
    );
    assert_eq!(
        dispatch(
            &mut provider,
            "http_response_read",
            vec![BoundaryValue::U64(body), BoundaryValue::U32(2)],
        ),
        BoundaryValue::Null
    );
    server.join().unwrap();
}

#[test]
fn websocket_provider_exchanges_loopback_message() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut socket = tungstenite::accept(stream).unwrap();
        assert_eq!(socket.read().unwrap().into_data(), b"ping");
        socket
            .send(tungstenite::Message::Binary(b"pong".to_vec()))
            .unwrap();
    });

    let mut provider = NativeWebSocketProvider::new();
    let socket = match dispatch(
        &mut provider,
        "websocket_connect",
        vec![BoundaryValue::Bytes(
            format!("ws://127.0.0.1:{port}").into_bytes(),
        )],
    ) {
        BoundaryValue::U64(id) => id,
        value => panic!("unexpected {value:?}"),
    };
    assert_eq!(
        dispatch(
            &mut provider,
            "websocket_send",
            vec![
                BoundaryValue::U64(socket),
                BoundaryValue::Bytes(b"ping".to_vec())
            ],
        ),
        BoundaryValue::Bool(true)
    );
    assert_eq!(
        dispatch(
            &mut provider,
            "websocket_receive",
            vec![BoundaryValue::U64(socket)],
        ),
        BoundaryValue::Bytes(b"pong".to_vec())
    );
    server.join().unwrap();
}

#[test]
fn tcp_and_udp_providers_exchange_loopback_bytes() {
    let tcp_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let tcp_port = tcp_listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let (mut stream, _) = tcp_listener.accept().unwrap();
        let mut data = [0; 4];
        stream.read_exact(&mut data).unwrap();
        assert_eq!(&data, b"ping");
        stream.write_all(b"pong").unwrap();
        let mut eof = [0; 1];
        assert_eq!(stream.read(&mut eof).unwrap(), 0);
    });

    let mut provider = NativeNetProvider::new();
    let socket = match dispatch(
        &mut provider,
        "net_tcp_connect",
        vec![
            BoundaryValue::Bytes(b"127.0.0.1".to_vec()),
            BoundaryValue::U16(tcp_port),
        ],
    ) {
        BoundaryValue::U64(id) => id,
        value => panic!("unexpected {value:?}"),
    };
    assert_eq!(
        dispatch(
            &mut provider,
            "net_tcp_write",
            vec![
                BoundaryValue::U64(socket),
                BoundaryValue::Bytes(b"ping".to_vec())
            ]
        ),
        BoundaryValue::Bool(true)
    );
    assert_eq!(
        dispatch(
            &mut provider,
            "net_tcp_read",
            vec![BoundaryValue::U64(socket), BoundaryValue::U32(32)]
        ),
        BoundaryValue::Bytes(b"pong".to_vec())
    );
    assert_eq!(
        dispatch(
            &mut provider,
            "net_tcp_finish",
            vec![BoundaryValue::U64(socket)]
        ),
        BoundaryValue::Bool(true)
    );

    let peer = UdpSocket::bind("127.0.0.1:0").unwrap();
    let peer_port = peer.local_addr().unwrap().port();
    let udp = match dispatch(
        &mut provider,
        "net_udp_bind",
        vec![
            BoundaryValue::Bytes(b"127.0.0.1".to_vec()),
            BoundaryValue::U16(0),
        ],
    ) {
        BoundaryValue::U64(id) => id,
        value => panic!("unexpected {value:?}"),
    };
    assert_eq!(
        dispatch(
            &mut provider,
            "net_udp_send_to",
            vec![
                BoundaryValue::U64(udp),
                BoundaryValue::Bytes(b"127.0.0.1".to_vec()),
                BoundaryValue::U16(peer_port),
                BoundaryValue::Bytes(b"ping".to_vec())
            ]
        ),
        BoundaryValue::Bool(true)
    );
    let mut buffer = [0; 4];
    let (size, address) = peer.recv_from(&mut buffer).unwrap();
    assert_eq!(&buffer[..size], b"ping");
    peer.send_to(b"pong", address).unwrap();
    let result = dispatch(
        &mut provider,
        "net_udp_receive",
        vec![BoundaryValue::U64(udp), BoundaryValue::U32(32)],
    );
    assert!(
        matches!(result, BoundaryValue::Tuple(values) if values[0] == BoundaryValue::Bytes(b"pong".to_vec()))
    );
}
