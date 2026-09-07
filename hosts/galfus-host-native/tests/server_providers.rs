use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use galfus_contract::{HostProvider, MessageInjectionError, MessageInjector, SurfaceValue};
use galfus_host_native::providers::server::NativeServerProvider;
use tungstenite::{Message, connect};

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
fn websocket_lifecycle_is_validated() {
    let mut provider = NativeServerProvider::new();
    let port: i32 = 45678;

    let server_id = match dispatch(&mut provider, "server_bind", vec![SurfaceValue::I32(port)]) {
        SurfaceValue::U64(id) => id,
        value => panic!("unexpected bind result: {value:?}"),
    };

    let client = std::thread::spawn(move || {
        // Allow server to bind
        std::thread::sleep(Duration::from_millis(100));
        let (mut socket, response) = connect(format!("ws://127.0.0.1:{port}/ws")).unwrap();
        assert_eq!(response.status(), 101);

        socket.send(Message::Text("ping".into())).unwrap();
        let msg = socket.read().unwrap();
        assert_eq!(msg.into_text().unwrap(), "pong");

        // Binary message
        socket.send(Message::Binary(b"bin_ping".to_vec())).unwrap();
        let msg2 = socket.read().unwrap();
        assert_eq!(msg2.into_data(), b"bin_pong");

        socket.close(None).unwrap();
    });

    let request = match dispatch(
        &mut provider,
        "server_accept",
        vec![SurfaceValue::U64(server_id)],
    ) {
        SurfaceValue::Struct(fields) => fields,
        value => panic!("unexpected accept result: {value:?}"),
    };

    let mut request_id = 0;
    for (k, v) in request {
        if k == "id"
            && let SurfaceValue::U64(id) = v
        {
            request_id = id;
        }
    }
    assert!(request_id > 0);

    // Accept websocket upgrade
    assert_eq!(
        dispatch(
            &mut provider,
            "server_response_start",
            vec![
                SurfaceValue::U64(request_id),
                SurfaceValue::I32(101),
                SurfaceValue::List(vec![]),
                SurfaceValue::Bool(true),
            ]
        ),
        SurfaceValue::Bool(true)
    );

    assert_eq!(
        dispatch(
            &mut provider,
            "server_response_finish",
            vec![SurfaceValue::U64(request_id)]
        ),
        SurfaceValue::Bool(true)
    );

    // Read message from client
    let msg = dispatch(
        &mut provider,
        "server_ws_receive",
        vec![SurfaceValue::U64(request_id)],
    );
    let (status, msg_bytes) = match msg {
        SurfaceValue::Struct(fields) => {
            let mut status = 0;
            let mut msg_bytes = None;
            for (k, v) in fields {
                if k == "status" {
                    if let SurfaceValue::I32(s) = v {
                        status = s;
                    }
                } else if k == "msg"
                    && let SurfaceValue::Bytes(b) = v
                {
                    msg_bytes = Some(b);
                }
            }
            (status, msg_bytes.unwrap())
        }
        _ => panic!("Expected WsMessage Struct"),
    };
    assert_eq!(status, 1); // 1 = text frame
    assert_eq!(msg_bytes, b"ping");

    // Send message to client
    assert_eq!(
        dispatch(
            &mut provider,
            "server_ws_send",
            vec![
                SurfaceValue::U64(request_id),
                SurfaceValue::Bytes(b"pong".to_vec())
            ]
        ),
        SurfaceValue::Bool(true)
    );

    // Read binary message from client
    let msg_bin = dispatch(
        &mut provider,
        "server_ws_receive",
        vec![SurfaceValue::U64(request_id)],
    );
    let (status_bin, msg_bytes_bin) = match msg_bin {
        SurfaceValue::Struct(fields) => {
            let mut status = 0;
            let mut msg_bytes = None;
            for (k, v) in fields {
                if k == "status" {
                    if let SurfaceValue::I32(s) = v {
                        status = s;
                    }
                } else if k == "msg"
                    && let SurfaceValue::Bytes(b) = v
                {
                    msg_bytes = Some(b);
                }
            }
            (status, msg_bytes.unwrap())
        }
        _ => panic!("Expected WsMessage Struct"),
    };
    assert_eq!(status_bin, 2); // 2 = binary frame
    assert_eq!(msg_bytes_bin, b"bin_ping");

    // Send binary message to client
    assert_eq!(
        dispatch(
            &mut provider,
            "server_ws_send",
            vec![
                SurfaceValue::U64(request_id),
                SurfaceValue::Bytes(b"bin_pong".to_vec())
            ]
        ),
        SurfaceValue::Bool(true)
    );

    // Read close frame (should return null struct payload for WS)
    let msg2 = dispatch(
        &mut provider,
        "server_ws_receive",
        vec![SurfaceValue::U64(request_id)],
    );
    println!("MSG2: {:?}", msg2);
    let (status2, msg_null) = match msg2 {
        SurfaceValue::Struct(fields) => {
            let mut status = 0;
            let mut is_null = false;
            for (k, v) in fields {
                if k == "status" {
                    if let SurfaceValue::I32(s) = v {
                        status = s;
                    }
                } else if k == "msg" && matches!(v, SurfaceValue::Null) {
                    is_null = true;
                }
            }
            (status, is_null)
        }
        _ => panic!("Expected WsMessage Struct"),
    };
    assert_eq!(status2, 1000); // 1000 = normal close
    assert!(msg_null);

    // Close the socket
    assert_eq!(
        dispatch(
            &mut provider,
            "server_ws_close",
            vec![SurfaceValue::U64(request_id)]
        ),
        SurfaceValue::Bool(true)
    );

    client.join().unwrap();
}

#[test]
fn websocket_transport_error_is_validated() {
    use std::net::Shutdown;

    let mut provider = NativeServerProvider::new();
    let port: i32 = 45679;

    let server_id = match dispatch(&mut provider, "server_bind", vec![SurfaceValue::I32(port)]) {
        SurfaceValue::U64(id) => id,
        value => panic!("unexpected bind result: {value:?}"),
    };

    let client = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        let (mut socket, _) = connect(format!("ws://127.0.0.1:{port}/ws")).unwrap();

        // Forcefully close the TCP connection without a WS Close frame
        match socket.get_mut() {
            tungstenite::stream::MaybeTlsStream::Plain(s) => {
                s.shutdown(Shutdown::Both).unwrap();
            }
            _ => panic!("Expected plain stream"),
        }
    });

    let request = match dispatch(
        &mut provider,
        "server_accept",
        vec![SurfaceValue::U64(server_id)],
    ) {
        SurfaceValue::Struct(fields) => fields,
        _ => panic!("unexpected accept result"),
    };

    let mut request_id = 0;
    for (k, v) in request {
        if k == "id"
            && let SurfaceValue::U64(id) = v
        {
            request_id = id;
        }
    }

    dispatch(
        &mut provider,
        "server_response_start",
        vec![
            SurfaceValue::U64(request_id),
            SurfaceValue::I32(101),
            SurfaceValue::List(vec![]),
            SurfaceValue::Bool(true),
        ],
    );

    dispatch(
        &mut provider,
        "server_response_finish",
        vec![SurfaceValue::U64(request_id)],
    );

    let err_msg = dispatch(
        &mut provider,
        "server_ws_receive",
        vec![SurfaceValue::U64(request_id)],
    );
    let status_err = match err_msg {
        SurfaceValue::Struct(fields) => {
            let mut status = 0;
            for (k, v) in fields {
                if k == "status"
                    && let SurfaceValue::I32(s) = v
                {
                    status = s;
                }
            }
            status
        }
        _ => panic!("Expected WsMessage Struct"),
    };
    assert_eq!(status_err, -1); // -1 = transport error

    dispatch(
        &mut provider,
        "server_ws_close",
        vec![SurfaceValue::U64(request_id)],
    );
    client.join().unwrap();
}

#[test]
fn provider_waiters_are_removed_on_cancellation() {
    let mut provider = NativeServerProvider::new();
    let port: i32 = 45680;

    let server_id = match dispatch(&mut provider, "server_bind", vec![SurfaceValue::I32(port)]) {
        SurfaceValue::U64(id) => id,
        value => panic!("unexpected bind result: {value:?}"),
    };

    let injector = Arc::new(Injector(Mutex::new(Vec::new())));
    let thread_id = galfus_core::ThreadId::new(2);
    let request_lease = galfus_core::RequestLease::new(galfus_core::RequestId::new(2), 0);

    // Dispatch an accept that will pend forever (since no client connects)
    let dispatched = provider.dispatch_surface(
        thread_id,
        request_lease,
        "server_accept",
        &[SurfaceValue::U64(server_id)],
        injector.clone(),
    );
    assert!(dispatched);

    // Now we cancel the lease
    let outcome = provider.cancel(thread_id, request_lease);
    assert!(matches!(
        outcome,
        galfus_contract::CancellationOutcome::BestEffort
    ));

    // Wait briefly to let the background thread process the cancellation
    std::thread::sleep(Duration::from_millis(50));

    // If the waiter was removed, another client connecting now should NOT resolve the old waiter.
    // However, since it's hard to observe the internal waiter queue directly without a leak detector,
    // we can trust the coverage or use the client to connect and see if the old injector gets a response.
    let client = std::thread::spawn(move || {
        let _socket = std::net::TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
    });

    std::thread::sleep(Duration::from_millis(100));
    // The old injector should STILL be empty because it was cancelled!
    assert!(injector.0.lock().unwrap().is_empty());

    client.join().unwrap();
}
