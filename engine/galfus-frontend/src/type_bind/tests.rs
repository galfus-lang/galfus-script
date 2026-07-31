use super::*;
use crate::{ModuleGraph, PrimitiveType, SyntaxNodeKind, TypeKind, parse, resolve};
use galfus_core::{NodeId, SourceFile};

fn source(text: &str) -> SourceFile {
    SourceFile::new(
        galfus_core::SourceId::new(0),
        "test.gfs".to_string(),
        text.to_string(),
    )
}

fn find_node_by_kind_and_text(
    source: &SourceFile,
    graph: &ModuleGraph,
    kind: SyntaxNodeKind,
    text: &str,
) -> Option<NodeId> {
    let root = graph.syntax().root()?;
    find_node_by_kind_and_text_from(source, graph, root, kind, text)
}

fn find_node_by_kind_and_text_from(
    source: &SourceFile,
    graph: &ModuleGraph,
    node: NodeId,
    kind: SyntaxNodeKind,
    text: &str,
) -> Option<NodeId> {
    let syntax_node = graph.syntax().node(node)?;

    if syntax_node.kind() == kind && source.slice(syntax_node.span()) == Some(text) {
        return Some(node);
    }

    for child in syntax_node.children() {
        if let Some(found) = find_node_by_kind_and_text_from(source, graph, *child, kind, text) {
            return Some(found);
        }
    }

    None
}

#[test]
fn lower_binds_primitive_named_type() {
    let source = source(
        r#"
fn main(value: i32): null {
  return
}
"#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());
    assert!(!resolve_result.has_errors());

    let result = bind_types(&source, resolve_result.graph());
    let graph = resolve_result.graph();

    let int32_node =
        find_node_by_kind_and_text(&source, graph, SyntaxNodeKind::NamedType, "i32").unwrap();

    let ty = result.layer().node_type(int32_node).unwrap();

    assert_eq!(
        result.layer().table().kind(ty),
        Some(&TypeKind::Primitive(PrimitiveType::Int32))
    );
}

#[test]
fn lower_binds_array_type() {
    let source = source(
        r#"
fn main(values: [i32]): null {
  return
}
"#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());
    assert!(!resolve_result.has_errors());

    let result = bind_types(&source, resolve_result.graph());
    let graph = resolve_result.graph();

    let array_node =
        find_node_by_kind_and_text(&source, graph, SyntaxNodeKind::ArrayType, "[i32]").unwrap();

    let ty = result.layer().node_type(array_node).unwrap();

    match result.layer().table().kind(ty) {
        Some(TypeKind::Array { element }) => {
            assert_eq!(
                result.layer().table().kind(*element),
                Some(&TypeKind::Primitive(PrimitiveType::Int32))
            );
        }
        other => panic!("expected array type, got {other:?}"),
    }
}

#[test]
fn lower_normalizes_union_type() {
    let source = source(
        r#"
fn main(value: i32 | null | i32): null {
  return
}
"#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());
    assert!(!resolve_result.has_errors());

    let result = bind_types(&source, resolve_result.graph());
    let graph = resolve_result.graph();

    let union_node = find_node_by_kind_and_text(
        &source,
        graph,
        SyntaxNodeKind::UnionType,
        "i32 | null | i32",
    )
    .unwrap();

    let ty = result.layer().node_type(union_node).unwrap();

    match result.layer().table().kind(ty) {
        Some(TypeKind::Union { members }) => {
            assert_eq!(members.len(), 2);
        }
        other => panic!("expected union type, got {other:?}"),
    }
}

#[test]
fn lower_binds_named_struct_type() {
    let source = source(
        r#"
struct User {
  id: i64,
}

fn main(value: User): null {
  return
}
"#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());
    assert!(!resolve_result.has_errors());

    let result = bind_types(&source, resolve_result.graph());
    let graph = resolve_result.graph();

    let user_node =
        find_node_by_kind_and_text(&source, graph, SyntaxNodeKind::NamedType, "User").unwrap();

    let ty = result.layer().node_type(user_node).unwrap();

    assert!(matches!(
        result.layer().table().kind(ty),
        Some(TypeKind::Named { .. })
    ));
}

#[test]
fn lower_binds_external_path_type() {
    let source = source(
        r#"
import user from "./user"

fn main(value: user::User): null {
  return
}
"#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());
    assert!(!resolve_result.has_errors());

    let result = bind_types(&source, resolve_result.graph());
    let graph = resolve_result.graph();

    let path_node =
        find_node_by_kind_and_text(&source, graph, SyntaxNodeKind::Path, "user::User").unwrap();

    let ty = result.layer().node_type(path_node).unwrap();

    match result.layer().table().kind(ty) {
        Some(TypeKind::Path { segments, .. }) => {
            assert_eq!(segments, &vec!["User".to_string()]);
        }
        other => panic!("expected path type, got {other:?}"),
    }
}

#[test]
fn lower_binds_generic_instance_type() {
    let source = source(
        r#"
choice Result<V, E> {
  Ok(V),
  Err(E),
}

struct Error {
  code: i32,
}

fn main(value: Result<i32, Error>): null {
  return
}
"#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());
    assert!(!resolve_result.has_errors());

    let result = bind_types(&source, resolve_result.graph());
    let graph = resolve_result.graph();

    let generic_node = find_node_by_kind_and_text(
        &source,
        graph,
        SyntaxNodeKind::GenericType,
        "Result<i32, Error>",
    )
    .unwrap();

    let ty = result.layer().node_type(generic_node).unwrap();

    assert!(matches!(
        result.layer().table().kind(ty),
        Some(TypeKind::GenericInstance { .. })
    ));
}
