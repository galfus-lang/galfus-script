use super::*;

#[test]
fn resolve_partial_parse_graph_without_panic() {
    let source = source(
        "fn broken(): null {
            var value: =
            return
        }

        fn next(): null { return }",
    );

    let parse_result = parse(&source);
    assert!(parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());

    assert!(resolve_result.graph().resolution().is_some());
}
