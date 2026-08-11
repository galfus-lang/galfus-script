use super::EventSequence;

#[test]
fn event_sequences_are_strictly_monotonic() {
    let first = EventSequence::FIRST;
    let second = first.next().expect("sequence continues");

    assert!(second > first);
    assert_eq!(second.0, first.0 + 1);
}
