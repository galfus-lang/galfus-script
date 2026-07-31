use super::*;
use crate::SourceId;

#[test]
fn span_len_returns_distance_between_start_and_end() {
    let source_id = SourceId::new(0);
    let span = Span::new(source_id, 3, 8);

    assert_eq!(span.len(), 5);
}

#[test]
fn span_len_returns_zero_when_invalid() {
    let source_id = SourceId::new(0);
    let span = Span::new(source_id, 8, 3);

    assert_eq!(span.len(), 0);
}

#[test]
fn span_empty_creates_zero_length_span() {
    let source_id = SourceId::new(0);
    let span = Span::empty(source_id, 10);

    assert_eq!(span.start(), 10);
    assert_eq!(span.end(), 10);
    assert_eq!(span.len(), 0);
    assert!(span.is_empty());
}

#[test]
fn span_contains_uses_start_inclusive_end_exclusive() {
    let source_id = SourceId::new(0);
    let span = Span::new(source_id, 3, 8);

    assert!(!span.contains(2));
    assert!(span.contains(3));
    assert!(span.contains(7));
    assert!(!span.contains(8));
}

#[test]
fn checked_new_returns_some_for_valid_span() {
    let source_id = SourceId::new(0);
    let span = Span::checked_new(source_id, 3, 8);

    assert_eq!(span, Some(Span::new(source_id, 3, 8)));
}

#[test]
fn checked_new_returns_none_for_invalid_span() {
    let source_id = SourceId::new(0);
    let span = Span::checked_new(source_id, 8, 3);

    assert_eq!(span, None);
}

#[test]
fn cover_returns_span_covering_both_spans() {
    let source_id = SourceId::new(0);

    let left = Span::new(source_id, 3, 8);
    let right = Span::new(source_id, 10, 15);

    assert_eq!(left.cover(right), Some(Span::new(source_id, 3, 15)));
}

#[test]
fn cover_returns_none_for_different_sources() {
    let left = Span::new(SourceId::new(0), 3, 8);
    let right = Span::new(SourceId::new(1), 10, 15);

    assert_eq!(left.cover(right), None);
}
