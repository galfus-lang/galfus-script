use super::*;

#[test]
fn parse_type_alias_item() {
    let source = source("type UserId = i32");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let root_node = syntax.node(root).unwrap();

    assert_eq!(root_node.child_count(), 1);

    let item = root_node.first_child().unwrap();
    let item_node = syntax.node(item).unwrap();

    assert_eq!(item_node.kind(), SyntaxNodeKind::TypeAliasItem);
    assert_eq!(source.slice(item_node.span()), Some("type UserId = i32"));
    assert_eq!(item_node.child_count(), 2);

    let name = item_node.first_child().unwrap();
    let aliased_type = item_node.child(1).unwrap();

    assert_eq!(
        syntax.node(name).unwrap().kind(),
        SyntaxNodeKind::Identifier
    );
    assert_eq!(
        source.slice(syntax.node(name).unwrap().span()),
        Some("UserId")
    );

    assert_eq!(
        syntax.node(aliased_type).unwrap().kind(),
        SyntaxNodeKind::NamedType
    );
    assert_eq!(
        source.slice(syntax.node(aliased_type).unwrap().span()),
        Some("i32")
    );
}

#[test]
fn parse_type_alias_followed_by_function() {
    let source = source("type UserId = i32\nfn main(): null { return }");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let root_node = syntax.node(root).unwrap();

    assert_eq!(root_node.child_count(), 2);

    let alias = root_node.first_child().unwrap();
    let function = root_node.child(1).unwrap();

    assert_eq!(
        syntax.node(alias).unwrap().kind(),
        SyntaxNodeKind::TypeAliasItem
    );

    assert_eq!(
        syntax.node(function).unwrap().kind(),
        SyntaxNodeKind::FunctionItem
    );
}

#[test]
fn parse_export_type_alias_item() {
    let source = source("export type UserId = i32");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let root_node = syntax.node(root).unwrap();

    assert_eq!(root_node.child_count(), 1);

    let export = root_node.first_child().unwrap();
    let export_node = syntax.node(export).unwrap();

    assert_eq!(export_node.kind(), SyntaxNodeKind::ExportItem);
    assert_eq!(
        source.slice(export_node.span()),
        Some("export type UserId = i32")
    );
    assert_eq!(export_node.child_count(), 1);

    let inner = export_node.first_child().unwrap();
    let inner_node = syntax.node(inner).unwrap();

    assert_eq!(inner_node.kind(), SyntaxNodeKind::TypeAliasItem);
}

#[test]
fn parse_export_function_item() {
    let source = source("export fn main(): null { return }");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let root_node = syntax.node(root).unwrap();

    assert_eq!(root_node.child_count(), 1);

    let export = root_node.first_child().unwrap();
    let export_node = syntax.node(export).unwrap();

    assert_eq!(export_node.kind(), SyntaxNodeKind::ExportItem);
    assert_eq!(
        source.slice(export_node.span()),
        Some("export fn main(): null { return }")
    );
    assert_eq!(export_node.child_count(), 1);

    let inner = export_node.first_child().unwrap();
    let inner_node = syntax.node(inner).unwrap();

    assert_eq!(inner_node.kind(), SyntaxNodeKind::FunctionItem);
}

#[test]
fn parse_reports_invalid_export_item() {
    let source = source("export return");

    let result = parse(&source);

    assert!(result.has_errors());

    let diagnostic = result.diagnostics().iter().next().unwrap();

    assert_eq!(diagnostic.code().as_str(), "P0002");
    assert_eq!(
        diagnostic.message(),
        "expected exportable item, found `Return`"
    );
}
#[test]
fn parse_namespace_import_item() {
    let source = source("import math from \"std/math\"");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let import = syntax.node(root).unwrap().first_child().unwrap();
    let import_node = syntax.node(import).unwrap();

    let clause = import_node.first_child().unwrap();
    let clause_node = syntax.node(clause).unwrap();

    assert_eq!(clause_node.kind(), SyntaxNodeKind::NamespaceImport);

    let name = clause_node.first_child().unwrap();

    assert_eq!(
        source.slice(syntax.node(name).unwrap().span()),
        Some("math")
    );
}

#[test]
fn parse_named_import_list() {
    let source = source("import { sin, cos } from \"std/math\"");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let import = syntax.node(root).unwrap().first_child().unwrap();
    let import_node = syntax.node(import).unwrap();

    let clause = import_node.first_child().unwrap();
    let clause_node = syntax.node(clause).unwrap();

    assert_eq!(clause_node.kind(), SyntaxNodeKind::NamedImportList);
    assert_eq!(source.slice(clause_node.span()), Some("{ sin, cos }"));
    assert_eq!(clause_node.child_count(), 2);

    let first = clause_node.first_child().unwrap();
    let second = clause_node.child(1).unwrap();

    assert_eq!(
        syntax.node(first).unwrap().kind(),
        SyntaxNodeKind::NamedImport
    );
    assert_eq!(
        syntax.node(second).unwrap().kind(),
        SyntaxNodeKind::NamedImport
    );
}

#[test]
fn parse_named_import_list_recovers_missing_comma() {
    let source = source("import { sin cos } from \"std/math\"");

    let result = parse(&source);

    assert!(result.has_errors());
    assert!(result.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == "P0001"
            && diagnostic.message() == "expected `Comma`, found `Identifier`"
    }));

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();
    let import = syntax.node(root).unwrap().first_child().unwrap();
    let import_node = syntax.node(import).unwrap();
    let clause = import_node.first_child().unwrap();
    let clause_node = syntax.node(clause).unwrap();

    assert_eq!(clause_node.kind(), SyntaxNodeKind::NamedImportList);
    assert_eq!(clause_node.child_count(), 2);
}

#[test]
fn parse_named_import_alias() {
    let source = source("import { sin as sine, cos } from \"std/math\"");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let import = syntax.node(root).unwrap().first_child().unwrap();
    let import_node = syntax.node(import).unwrap();

    let clause = import_node.first_child().unwrap();
    let clause_node = syntax.node(clause).unwrap();

    assert_eq!(clause_node.kind(), SyntaxNodeKind::NamedImportList);
    assert_eq!(clause_node.child_count(), 2);

    let first_import = clause_node.first_child().unwrap();
    let first_import_node = syntax.node(first_import).unwrap();

    assert_eq!(first_import_node.kind(), SyntaxNodeKind::NamedImport);
    assert_eq!(source.slice(first_import_node.span()), Some("sin as sine"));
    assert_eq!(first_import_node.child_count(), 2);

    let imported_name = first_import_node.first_child().unwrap();
    let alias = first_import_node.child(1).unwrap();

    assert_eq!(
        source.slice(syntax.node(imported_name).unwrap().span()),
        Some("sin")
    );

    let alias_node = syntax.node(alias).unwrap();

    assert_eq!(alias_node.kind(), SyntaxNodeKind::ImportAlias);
    assert_eq!(source.slice(alias_node.span()), Some("as sine"));

    let local_name = alias_node.first_child().unwrap();

    assert_eq!(
        source.slice(syntax.node(local_name).unwrap().span()),
        Some("sine")
    );

    let second_import = clause_node.child(1).unwrap();
    let second_import_node = syntax.node(second_import).unwrap();

    assert_eq!(source.slice(second_import_node.span()), Some("cos"));
    assert_eq!(second_import_node.child_count(), 1);
}

#[test]
fn parse_export_constraint_item() {
    let source = source(
        "export constraint Stringable<T> {
            fn toString(self): [i8]
        }",
    );

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();
    let root_node = syntax.node(root).unwrap();

    assert_eq!(root_node.child_count(), 1);

    let export = root_node.first_child().unwrap();
    let export_node = syntax.node(export).unwrap();

    assert_eq!(export_node.kind(), SyntaxNodeKind::ExportItem);
    assert_eq!(export_node.child_count(), 1);

    let inner = export_node.first_child().unwrap();
    let inner_node = syntax.node(inner).unwrap();

    assert_eq!(inner_node.kind(), SyntaxNodeKind::ConstraintItem);
}

#[test]
fn parse_export_rejects_non_item() {
    let source = source("export return");

    let result = parse(&source);

    assert!(result.has_errors());
}

#[test]
fn parse_export_rejects_expression() {
    let source = source("export 123");

    let result = parse(&source);

    assert!(result.has_errors());
}
