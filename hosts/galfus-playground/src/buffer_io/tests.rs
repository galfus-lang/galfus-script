use super::*;
use std::sync::{Arc, Mutex};

struct MockInjector {
    response: Arc<
        Mutex<
            Option<(
                galfus_core::RequestId,
                Result<BoundaryValue, ExecutionFailure>,
            )>,
        >,
    >,
}

impl MessageInjector for MockInjector {
    fn inject_system_response(
        &self,
        _thread_id: galfus_core::ThreadId,
        request_lease: galfus_core::RequestLease,
        response: Result<BoundaryValue, ExecutionFailure>,
    ) {
        *self.response.lock().unwrap() = Some((request_lease.id, response));
    }
}

fn call_dispatch(
    provider: &mut BufferIoProvider,
    method: &str,
    args: &[BoundaryValue],
) -> Option<Result<BoundaryValue, ExecutionFailure>> {
    let response = Arc::new(Mutex::new(None));
    let injector = Arc::new(MockInjector {
        response: Arc::clone(&response),
    });
    provider.dispatch(
        galfus_core::ThreadId::new(0),
        galfus_core::RequestLease::new(galfus_core::RequestId::new(1), 1),
        method,
        args,
        injector,
    );
    response
        .lock()
        .unwrap()
        .take()
        .map(|(_, response)| response)
}

#[test]
fn reads_until_terminator_and_keeps_remaining_input() {
    let mut provider = BufferIoProvider::new(b"first\r\nsecond".to_vec());

    assert_eq!(
        call_dispatch(
            &mut provider,
            "read",
            &[BoundaryValue::Bytes(b"\r\n".to_vec())]
        ),
        Some(Ok(BoundaryValue::Bytes(b"first".to_vec())))
    );
    // "second" doesn't have a terminator, so it blocks
    assert_eq!(
        call_dispatch(
            &mut provider,
            "read",
            &[BoundaryValue::Bytes(b"\r\n".to_vec())]
        ),
        None
    );
}

#[test]
fn captures_written_output() {
    let mut provider = BufferIoProvider::default();

    assert_eq!(
        call_dispatch(
            &mut provider,
            "write",
            &[BoundaryValue::Bytes(b"hello".to_vec())],
        ),
        Some(Ok(BoundaryValue::Null))
    );
    assert_eq!(
        call_dispatch(
            &mut provider,
            "write",
            &[BoundaryValue::Bytes(b" world".to_vec())],
        ),
        Some(Ok(BoundaryValue::Null))
    );

    assert_eq!(provider.take_output(), b"hello world");
    assert_eq!(provider.take_output(), b"");
}

#[test]
fn rejects_an_empty_terminator() {
    let mut provider = BufferIoProvider::default();
    let error =
        call_dispatch(&mut provider, "read", &[BoundaryValue::Bytes(b"".to_vec())]).unwrap();

    assert!(matches!(error, Err(e) if e.message == "input terminator must not be empty"));
}

#[test]
fn pending_reads_preserve_the_provider_request_id() {
    let mut provider = BufferIoProvider::default();
    let response = Arc::new(Mutex::new(None));
    let injector = Arc::new(MockInjector {
        response: Arc::clone(&response),
    });

    provider.dispatch(
        galfus_core::ThreadId::new(7),
        galfus_core::RequestLease::new(galfus_core::RequestId::new(42), 1),
        "read",
        &[BoundaryValue::Bytes(b"\n".to_vec())],
        injector,
    );
    provider.send_read_data(b"value\n");

    assert_eq!(
        response.lock().unwrap().take(),
        Some((
            galfus_core::RequestId::new(42),
            Ok(BoundaryValue::Bytes(b"value".to_vec()))
        ))
    );
}

#[test]
fn receives_read_data_after_creation() {
    let mut provider = BufferIoProvider::default();
    provider.send_read_data(b"input\n");

    assert_eq!(
        call_dispatch(
            &mut provider,
            "read",
            &[BoundaryValue::Bytes(b"\n".to_vec())]
        ),
        Some(Ok(BoundaryValue::Bytes(b"input".to_vec())))
    );
}
