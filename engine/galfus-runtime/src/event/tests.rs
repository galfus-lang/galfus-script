use super::*;
use std::thread;

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

#[test]
fn concurrent_producers_are_serialized_by_event_id() {
    let (sender, receiver) = mpsc::channel();
    let sink = EventSink::new(sender);
    let producers = (0..4)
        .map(|_| {
            let sink = sink.clone();
            thread::spawn(move || {
                for _ in 0..8 {
                    sink.send(RuntimeEvent::Tick { delta_ms: 1 });
                }
            })
        })
        .collect::<Vec<_>>();

    for producer in producers {
        producer.join().expect("event producer completes");
    }

    let event_ids = (0..32)
        .map(|_| receiver.recv().expect("event is queued").0)
        .collect::<Vec<_>>();
    assert_eq!(event_ids, (1..=32).collect::<Vec<_>>());
}
