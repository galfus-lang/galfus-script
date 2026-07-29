use super::*;

#[test]
fn events_receive_monotonic_ids_in_send_order() {
    let (sender, receiver) = mpsc::channel();
    let sink = EventSink::new(sender);

    sink.send(RuntimeEvent::Tick { delta_ms: 1 });
    sink.send(RuntimeEvent::Tick { delta_ms: 2 });

    let (first_id, _) = receiver.recv().expect("first event is queued");
    let (second_id, _) = receiver.recv().expect("second event is queued");
    assert_eq!(first_id + 1, second_id);
}
