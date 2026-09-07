use galfus_contract::{HostProvider, MessageInjectionError, MessageInjector, SurfaceValue};
use galfus_host_native::providers::{
    http::NativeHttpProvider, net::NativeNetProvider, websocket::NativeWebSocketProvider,
};
use std::io::{Read, Write};
use std::net::{TcpListener, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct Injector(Mutex<Vec<SurfaceValue>>);

impl MessageInjector for Injector {
    fn inject_system_response(
        &self,
        _thread: galfus_core::ThreadId,
        _lease: galfus_core::RequestLease,
        result: Result<SurfaceValue, galfus_contract::ExecutionFailure>,
    ) -> Result<(), MessageInjectionError> {
        self.0.lock().unwrap().push(result.unwrap());
        Ok(())
    }

    fn inject_surface_response(
        &self,
        thread: galfus_core::ThreadId,
        lease: galfus_core::RequestLease,
        result: Result<SurfaceValue, galfus_contract::ExecutionFailure>,
    ) -> Result<(), MessageInjectionError> {
        self.inject_system_response(thread, lease, result)
    }
}

fn dispatch<P: HostProvider>(
    provider: &mut P,
    name: &str,
    args: Vec<SurfaceValue>,
) -> SurfaceValue {
    let injector = Arc::new(Injector(Mutex::new(Vec::new())));
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
        let mut request = Vec::new();
        let mut buf = [0; 512];
        loop {
            let size = stream.read(&mut buf).unwrap();
            request.extend_from_slice(&buf[..size]);
            if std::str::from_utf8(&request).unwrap_or("").contains("ping") {
                break;
            }
        }
        assert!(
            std::str::from_utf8(&request)
                .unwrap()
                .starts_with("POST /echo HTTP/1.1")
        );
        stream
            .write_all(b"HTTP/1.1 201 Created\r\nX-Test: yes\r\nContent-Length: 4\r\n\r\npong")
            .unwrap();
    });

    let mut provider = NativeHttpProvider::new();
    let req_id = match dispatch(
        &mut provider,
        "http_request_open",
        vec![
            SurfaceValue::Bytes(b"POST".to_vec()),
            SurfaceValue::Bytes(format!("http://127.0.0.1:{port}/echo").into_bytes()),
            SurfaceValue::List(vec![SurfaceValue::Struct(vec![
                (
                    "name".to_string(),
                    SurfaceValue::Bytes(b"X-Request".to_vec()),
                ),
                ("value".to_string(), SurfaceValue::Bytes(b"value".to_vec())),
            ])]),
        ],
    ) {
        SurfaceValue::U64(id) => id,
        value => panic!("unexpected req id {value:?}"),
    };

    assert_eq!(
        dispatch(
            &mut provider,
            "http_request_write",
            vec![
                SurfaceValue::U64(req_id),
                SurfaceValue::Bytes(b"ping".to_vec()),
            ]
        ),
        SurfaceValue::Bool(true)
    );

    let response = dispatch(
        &mut provider,
        "http_request_finish",
        vec![SurfaceValue::U64(req_id)],
    );
    let body = match response {
        SurfaceValue::Struct(fields)
            if fields[0] == ("status".to_string(), SurfaceValue::I32(201)) =>
        {
            match &fields[2] {
                (_, SurfaceValue::U64(body)) => *body,
                value => panic!("unexpected HTTP body handle {value:?}"),
            }
        }
        value => panic!("unexpected HTTP response {value:?}"),
    };
    assert_eq!(
        dispatch(
            &mut provider,
            "http_response_read",
            vec![SurfaceValue::U64(body), SurfaceValue::U32(2)],
        ),
        SurfaceValue::Bytes(b"po".to_vec())
    );
    assert_eq!(
        dispatch(
            &mut provider,
            "http_response_read",
            vec![SurfaceValue::U64(body), SurfaceValue::U32(2)],
        ),
        SurfaceValue::Bytes(b"ng".to_vec())
    );
    assert_eq!(
        dispatch(
            &mut provider,
            "http_response_read",
            vec![SurfaceValue::U64(body), SurfaceValue::U32(2)],
        ),
        SurfaceValue::Null
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
        vec![SurfaceValue::Bytes(
            format!("ws://127.0.0.1:{port}").into_bytes(),
        )],
    ) {
        SurfaceValue::U64(id) => id,
        value => panic!("unexpected {value:?}"),
    };
    assert_eq!(
        dispatch(
            &mut provider,
            "websocket_send",
            vec![
                SurfaceValue::U64(socket),
                SurfaceValue::Bytes(b"ping".to_vec())
            ],
        ),
        SurfaceValue::Bool(true)
    );
    assert_eq!(
        dispatch(
            &mut provider,
            "websocket_receive",
            vec![SurfaceValue::U64(socket)]
        ),
        SurfaceValue::Bytes(b"pong".to_vec())
    );
    server.join().unwrap();
}

#[test]
fn tcp_provider_reads_partially_and_handles_eof() {
    let tcp_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let tcp_port = tcp_listener.local_addr().unwrap().port();

    let server = std::thread::spawn(move || {
        let (mut stream, _) = tcp_listener.accept().unwrap();
        // Server receives 4 bytes
        let mut data = [0; 4];
        stream.read_exact(&mut data).unwrap();
        assert_eq!(&data, b"ping");

        // Server writes 6 bytes in two chunks but we want the client to read partially
        stream.write_all(b"12").unwrap();
        stream.write_all(b"3456").unwrap();

        // Wait for client half-close
        let mut eof = [0; 1];
        assert_eq!(stream.read(&mut eof).unwrap(), 0);
    });

    let mut provider = NativeNetProvider::new();
    let socket = match dispatch(
        &mut provider,
        "net_tcp_connect",
        vec![
            SurfaceValue::Bytes(b"127.0.0.1".to_vec()),
            SurfaceValue::U16(tcp_port),
        ],
    ) {
        SurfaceValue::U64(id) => id,
        value => panic!("unexpected {value:?}"),
    };

    assert_eq!(
        dispatch(
            &mut provider,
            "net_tcp_write",
            vec![
                SurfaceValue::U64(socket),
                SurfaceValue::Bytes(b"ping".to_vec())
            ],
        ),
        SurfaceValue::Bool(true)
    );

    // Read first 2 bytes (partial read, max_bytes = 2)
    assert_eq!(
        dispatch(
            &mut provider,
            "net_tcp_read",
            vec![SurfaceValue::U64(socket), SurfaceValue::U32(2)],
        ),
        SurfaceValue::Bytes(b"12".to_vec())
    );

    // Read next 3 bytes
    assert_eq!(
        dispatch(
            &mut provider,
            "net_tcp_read",
            vec![SurfaceValue::U64(socket), SurfaceValue::U32(3)],
        ),
        SurfaceValue::Bytes(b"345".to_vec())
    );

    // Read last byte
    assert_eq!(
        dispatch(
            &mut provider,
            "net_tcp_read",
            vec![SurfaceValue::U64(socket), SurfaceValue::U32(10)],
        ),
        SurfaceValue::Bytes(b"6".to_vec())
    );

    // Finish (half-close)
    assert_eq!(
        dispatch(
            &mut provider,
            "net_tcp_finish",
            vec![SurfaceValue::U64(socket)]
        ),
        SurfaceValue::Bool(true)
    );

    // Read EOF
    assert_eq!(
        dispatch(
            &mut provider,
            "net_tcp_read",
            vec![SurfaceValue::U64(socket), SurfaceValue::U32(10)],
        ),
        SurfaceValue::Null
    );

    server.join().unwrap();
}

#[test]
fn tcp_provider_cancellation_is_idempotent() {
    let tcp_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let tcp_port = tcp_listener.local_addr().unwrap().port();

    let server = std::thread::spawn(move || {
        let (mut stream, _) = tcp_listener.accept().unwrap();
        // Just wait until connection resets or closes
        let mut data = [0; 4];
        let _ = stream.read(&mut data);
    });

    let mut provider = NativeNetProvider::new();
    let socket = match dispatch(
        &mut provider,
        "net_tcp_connect",
        vec![
            SurfaceValue::Bytes(b"127.0.0.1".to_vec()),
            SurfaceValue::U16(tcp_port),
        ],
    ) {
        SurfaceValue::U64(id) => id,
        value => panic!("unexpected {value:?}"),
    };

    // Close the socket
    assert_eq!(
        dispatch(
            &mut provider,
            "net_tcp_close",
            vec![SurfaceValue::U64(socket)]
        ),
        SurfaceValue::Bool(true)
    );

    // Idempotent/safe failure on second close
    assert_eq!(
        dispatch(
            &mut provider,
            "net_tcp_close",
            vec![SurfaceValue::U64(socket)]
        ),
        SurfaceValue::Bool(false)
    );

    // Operations on closed socket fail gracefully
    assert_eq!(
        dispatch(
            &mut provider,
            "net_tcp_write",
            vec![
                SurfaceValue::U64(socket),
                SurfaceValue::Bytes(b"ping".to_vec())
            ],
        ),
        SurfaceValue::Bool(false)
    );

    assert_eq!(
        dispatch(
            &mut provider,
            "net_tcp_read",
            vec![SurfaceValue::U64(socket), SurfaceValue::U32(32)],
        ),
        SurfaceValue::Null
    );

    server.join().unwrap();
}

#[test]
fn udp_provider_preserves_message_boundaries() {
    let peer = UdpSocket::bind("127.0.0.1:0").unwrap();
    let peer_port = peer.local_addr().unwrap().port();

    let mut provider = NativeNetProvider::new();
    let udp = match dispatch(
        &mut provider,
        "net_udp_bind",
        vec![
            SurfaceValue::Bytes(b"127.0.0.1".to_vec()),
            SurfaceValue::U16(0),
        ],
    ) {
        SurfaceValue::U64(id) => id,
        value => panic!("unexpected {value:?}"),
    };

    assert_eq!(
        dispatch(
            &mut provider,
            "net_udp_send_to",
            vec![
                SurfaceValue::U64(udp),
                SurfaceValue::Bytes(b"127.0.0.1".to_vec()),
                SurfaceValue::U16(peer_port),
                SurfaceValue::Bytes(b"ping1".to_vec()),
            ],
        ),
        SurfaceValue::Bool(true)
    );

    assert_eq!(
        dispatch(
            &mut provider,
            "net_udp_send_to",
            vec![
                SurfaceValue::U64(udp),
                SurfaceValue::Bytes(b"127.0.0.1".to_vec()),
                SurfaceValue::U16(peer_port),
                SurfaceValue::Bytes(b"ping22".to_vec()),
            ],
        ),
        SurfaceValue::Bool(true)
    );

    let mut buffer = [0; 10];
    // First datagram
    let (size1, address) = peer.recv_from(&mut buffer).unwrap();
    assert_eq!(&buffer[..size1], b"ping1");
    // Second datagram
    let (size2, _) = peer.recv_from(&mut buffer).unwrap();
    assert_eq!(&buffer[..size2], b"ping22");

    // Send back datagrams
    peer.send_to(b"pong_A", address).unwrap();
    peer.send_to(b"pong_B", address).unwrap();

    // Receive first
    let result1 = dispatch(
        &mut provider,
        "net_udp_receive",
        vec![SurfaceValue::U64(udp), SurfaceValue::U32(32)],
    );
    assert!(
        matches!(result1, SurfaceValue::Tuple(values) if values[0] == SurfaceValue::Bytes(b"pong_A".to_vec()))
    );

    // Receive second
    let result2 = dispatch(
        &mut provider,
        "net_udp_receive",
        vec![SurfaceValue::U64(udp), SurfaceValue::U32(32)],
    );
    assert!(
        matches!(result2, SurfaceValue::Tuple(values) if values[0] == SurfaceValue::Bytes(b"pong_B".to_vec()))
    );

    // Close
    assert_eq!(
        dispatch(&mut provider, "net_udp_close", vec![SurfaceValue::U64(udp)]),
        SurfaceValue::Bool(true)
    );
}
