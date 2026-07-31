use super::*;
use crate::type_validation::check_definition_types;

#[test]
fn check_partial_parse_graph_without_panic() {
    let source = source(
        "fn broken(): i32 {
            return true +
        }

        fn next(): null { return }",
    );

    let parse_result = parse(&source);
    let has_parse_errors = parse_result.has_errors();
    let resolve_result = resolve(&source, parse_result.into_graph());
    let has_resolve_errors = resolve_result.has_errors();
    let graph = resolve_result.into_graph();
    let result = check_declaration_types(&source, &graph);
    let result = check_definition_types(&source, &graph, result);

    assert!(has_parse_errors || has_resolve_errors || result.has_errors());
}
