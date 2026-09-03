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
            SurfaceValue::Bytes(b"POST".to_vec()),
            SurfaceValue::Bytes(format!("http://127.0.0.1:{port}/echo").into_bytes()),
            SurfaceValue::List(vec![SurfaceValue::Struct(vec![
                (
                    "name".to_string(),
                    SurfaceValue::Bytes(b"X-Request".to_vec()),
                ),
                ("value".to_string(), SurfaceValue::Bytes(b"value".to_vec())),
            ])]),
            SurfaceValue::Bytes(b"ping".to_vec()),
        ],
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
    assert_eq!(
        dispatch(
            &mut provider,
            "net_tcp_read",
            vec![SurfaceValue::U64(socket), SurfaceValue::U32(32)],
        ),
        SurfaceValue::Bytes(b"pong".to_vec())
    );
    assert_eq!(
        dispatch(
            &mut provider,
            "net_tcp_finish",
            vec![SurfaceValue::U64(socket)]
        ),
        SurfaceValue::Bool(true)
    );

    let peer = UdpSocket::bind("127.0.0.1:0").unwrap();
    let peer_port = peer.local_addr().unwrap().port();
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
                SurfaceValue::Bytes(b"ping".to_vec()),
            ],
        ),
        SurfaceValue::Bool(true)
    );
    let mut buffer = [0; 4];
    let (size, address) = peer.recv_from(&mut buffer).unwrap();
    assert_eq!(&buffer[..size], b"ping");
    peer.send_to(b"pong", address).unwrap();
    let result = dispatch(
        &mut provider,
        "net_udp_receive",
        vec![SurfaceValue::U64(udp), SurfaceValue::U32(32)],
    );
    assert!(
        matches!(result, SurfaceValue::Tuple(values) if values[0] == SurfaceValue::Bytes(b"pong".to_vec()))
    );
}
