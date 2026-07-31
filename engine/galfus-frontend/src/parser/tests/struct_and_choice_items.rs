use super::*;
use crate::BinaryOperatorKind;

#[test]
fn parse_struct_item() {
    let source = source("struct User { name: [i8], age: i32 }");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let root_node = syntax.node(root).unwrap();

    assert_eq!(root_node.child_count(), 1);

    let item = root_node.first_child().unwrap();
    let item_node = syntax.node(item).unwrap();

    assert_eq!(item_node.kind(), SyntaxNodeKind::StructItem);
    assert_eq!(item_node.child_count(), 2);

    let name = item_node.first_child().unwrap();
    let fields = item_node.child(1).unwrap();

    assert_eq!(
        source.slice(syntax.node(name).unwrap().span()),
        Some("User")
    );

    let fields_node = syntax.node(fields).unwrap();

    assert_eq!(fields_node.kind(), SyntaxNodeKind::StructFieldList);
    assert_eq!(fields_node.child_count(), 2);

    let first_field = fields_node.first_child().unwrap();
    let first_field_node = syntax.node(first_field).unwrap();

    assert_eq!(first_field_node.kind(), SyntaxNodeKind::StructField);
    assert_eq!(source.slice(first_field_node.span()), Some("name: [i8]"));

    let first_field_name = first_field_node.first_child().unwrap();
    let first_field_type = first_field_node.child(1).unwrap();

    assert_eq!(
        source.slice(syntax.node(first_field_name).unwrap().span()),
        Some("name")
    );

    assert_eq!(
        syntax.node(first_field_type).unwrap().kind(),
        SyntaxNodeKind::ArrayType
    );

    assert_eq!(
        source.slice(syntax.node(first_field_type).unwrap().span()),
        Some("[i8]")
    );
}

#[test]
fn parse_struct_fields_with_commas() {
    let source = source("struct User { name: [i8], age: i32, }");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let item = syntax.node(root).unwrap().first_child().unwrap();
    let item_node = syntax.node(item).unwrap();

    let fields = item_node.child(1).unwrap();
    let fields_node = syntax.node(fields).unwrap();

    assert_eq!(fields_node.kind(), SyntaxNodeKind::StructFieldList);
    assert_eq!(fields_node.child_count(), 2);

    let second_field = fields_node.child(1).unwrap();

    assert_eq!(
        source.slice(syntax.node(second_field).unwrap().span()),
        Some("age: i32")
    );
}

#[test]
fn parse_struct_followed_by_function() {
    let source = source("struct User { name: [i8] }\nfn main(): null { return }");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let root_node = syntax.node(root).unwrap();

    assert_eq!(root_node.child_count(), 2);

    let struct_item = root_node.first_child().unwrap();
    let function_item = root_node.child(1).unwrap();

    assert_eq!(
        syntax.node(struct_item).unwrap().kind(),
        SyntaxNodeKind::StructItem
    );

    assert_eq!(
        syntax.node(function_item).unwrap().kind(),
        SyntaxNodeKind::FunctionItem
    );
}

#[test]
fn parse_export_struct_item() {
    let source = source("export struct User { name: [i8] }");

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
        Some("export struct User { name: [i8] }")
    );
    assert_eq!(export_node.child_count(), 1);

    let inner = export_node.first_child().unwrap();
    let inner_node = syntax.node(inner).unwrap();

    assert_eq!(inner_node.kind(), SyntaxNodeKind::StructItem);

    let name = inner_node.first_child().unwrap();

    assert_eq!(
        source.slice(syntax.node(name).unwrap().span()),
        Some("User")
    );
}

#[test]
fn parse_struct_requires_commas_between_fields() {
    let source = source("struct User { name: [i8] age: i32 }");

    let result = parse(&source);

    assert!(result.has_errors());

    let diagnostic = result.diagnostics().iter().next().unwrap();

    assert_eq!(diagnostic.code().as_str(), "P0001");
    assert_eq!(diagnostic.message(), "expected `Comma`, found `Identifier`");
}

#[test]
fn parse_enum_requires_commas_between_variants() {
    let source = source("enum Direction { North South }");

    let result = parse(&source);

    assert!(result.has_errors());

    let diagnostic = result.diagnostics().iter().next().unwrap();

    assert_eq!(diagnostic.code().as_str(), "P0001");
    assert_eq!(diagnostic.message(), "expected `Comma`, found `Identifier`");
}

#[test]
fn parse_choice_item_with_payload_variants() {
    let source = source("choice Result { Ok(User), SomeError(i32, [i8]), }");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let item = syntax.node(root).unwrap().first_child().unwrap();
    let item_node = syntax.node(item).unwrap();

    assert_eq!(item_node.kind(), SyntaxNodeKind::ChoiceItem);
    assert_eq!(item_node.child_count(), 2);

    let name = item_node.first_child().unwrap();
    let variants = item_node.child(1).unwrap();

    assert_eq!(
        source.slice(syntax.node(name).unwrap().span()),
        Some("Result")
    );

    let variants_node = syntax.node(variants).unwrap();

    assert_eq!(variants_node.kind(), SyntaxNodeKind::ChoiceVariantList);
    assert_eq!(variants_node.child_count(), 2);

    let ok_variant = variants_node.first_child().unwrap();
    let ok_variant_node = syntax.node(ok_variant).unwrap();

    assert_eq!(ok_variant_node.kind(), SyntaxNodeKind::ChoiceVariant);
    assert_eq!(source.slice(ok_variant_node.span()), Some("Ok(User)"));
    assert_eq!(ok_variant_node.child_count(), 2);

    let payload = ok_variant_node.child(1).unwrap();
    let payload_node = syntax.node(payload).unwrap();

    assert_eq!(payload_node.kind(), SyntaxNodeKind::ChoicePayload);
    assert_eq!(payload_node.child_count(), 1);

    let payload_item = payload_node.first_child().unwrap();
    let payload_item_node = syntax.node(payload_item).unwrap();

    assert_eq!(payload_item_node.kind(), SyntaxNodeKind::ChoicePayloadItem);
    assert_eq!(payload_item_node.child_count(), 1);

    let payload_type = payload_item_node.first_child().unwrap();

    assert_eq!(
        syntax.node(payload_type).unwrap().kind(),
        SyntaxNodeKind::NamedType
    );

    assert_eq!(
        source.slice(syntax.node(payload_type).unwrap().span()),
        Some("User")
    );
}

#[test]
fn parse_choice_variant_with_multiple_payload_types() {
    let source = source("choice Result { SomeError(i32, [i8]), }");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let item = syntax.node(root).unwrap().first_child().unwrap();
    let item_node = syntax.node(item).unwrap();

    let variants = item_node.child(1).unwrap();
    let variant = syntax.node(variants).unwrap().first_child().unwrap();
    let variant_node = syntax.node(variant).unwrap();

    assert_eq!(
        source.slice(variant_node.span()),
        Some("SomeError(i32, [i8])")
    );

    let payload = variant_node.child(1).unwrap();
    let payload_node = syntax.node(payload).unwrap();

    assert_eq!(payload_node.kind(), SyntaxNodeKind::ChoicePayload);
    assert_eq!(payload_node.child_count(), 2);

    let first_item = payload_node.first_child().unwrap();
    let second_item = payload_node.child(1).unwrap();

    assert_eq!(
        syntax.node(first_item).unwrap().kind(),
        SyntaxNodeKind::ChoicePayloadItem
    );
    assert_eq!(
        syntax.node(second_item).unwrap().kind(),
        SyntaxNodeKind::ChoicePayloadItem
    );

    let first_type = syntax.node(first_item).unwrap().first_child().unwrap();
    let second_type = syntax.node(second_item).unwrap().first_child().unwrap();

    assert_eq!(
        source.slice(syntax.node(first_type).unwrap().span()),
        Some("i32")
    );

    assert_eq!(
        source.slice(syntax.node(second_type).unwrap().span()),
        Some("[i8]")
    );
}

#[test]
fn parse_choice_item_with_unit_variant() {
    let source = source("choice Option { Some(User), None, }");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let item = syntax.node(root).unwrap().first_child().unwrap();
    let item_node = syntax.node(item).unwrap();

    let variants = item_node.child(1).unwrap();
    let variants_node = syntax.node(variants).unwrap();

    let none_variant = variants_node.child(1).unwrap();
    let none_variant_node = syntax.node(none_variant).unwrap();

    assert_eq!(none_variant_node.kind(), SyntaxNodeKind::ChoiceVariant);
    assert_eq!(source.slice(none_variant_node.span()), Some("None"));
    assert_eq!(none_variant_node.child_count(), 1);
}

#[test]
fn parse_generic_choice_item() {
    let source = source(
        "choice Result<V, F> {
            Ok(V),
            Err(F),
        }",
    );

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();
    let choice = syntax.first_child(root).unwrap();
    let choice_node = syntax.node(choice).unwrap();

    assert_eq!(choice_node.kind(), SyntaxNodeKind::ChoiceItem);
    assert_eq!(choice_node.child_count(), 3);

    let name = choice_node.child(0).unwrap();
    let generics = choice_node.child(1).unwrap();
    let variants = choice_node.child(2).unwrap();

    assert_eq!(
        syntax.node(name).unwrap().kind(),
        SyntaxNodeKind::Identifier
    );
    assert_eq!(
        syntax.node(generics).unwrap().kind(),
        SyntaxNodeKind::GenericParameterList
    );
    assert_eq!(
        syntax.node(variants).unwrap().kind(),
        SyntaxNodeKind::ChoiceVariantList
    );
}

#[test]
fn parse_enum_with_base_type() {
    let source = source(
        "enum(i64) TextureType {
            Float32,
            Float64,
        }",
    );

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();
    let enum_item = syntax.first_child(root).unwrap();
    let enum_node = syntax.node(enum_item).unwrap();

    assert_eq!(enum_node.kind(), SyntaxNodeKind::EnumItem);
    assert_eq!(enum_node.child_count(), 3);

    let base_type = enum_node.child(0).unwrap();
    let name = enum_node.child(1).unwrap();
    let variants = enum_node.child(2).unwrap();

    assert!(syntax.node(base_type).unwrap().kind().is_type());
    assert_eq!(
        source.slice(syntax.node(base_type).unwrap().span()),
        Some("i64")
    );
    assert_eq!(
        syntax.node(name).unwrap().kind(),
        SyntaxNodeKind::Identifier
    );
    assert_eq!(
        syntax.node(variants).unwrap().kind(),
        SyntaxNodeKind::EnumVariantList
    );
}

#[test]
fn parse_enum_variant_with_numeric_discriminant() {
    let source = source(
        "enum State {
            Off(1),
            On(2),
        }",
    );

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();

    let enum_item = syntax.first_child(root).unwrap();

    let variants = syntax
        .first_child_of_kind(enum_item, SyntaxNodeKind::EnumVariantList)
        .unwrap();

    let first_variant = syntax.child(variants, 0).unwrap();
    let first_variant_node = syntax.node(first_variant).unwrap();

    assert_eq!(first_variant_node.kind(), SyntaxNodeKind::EnumVariant);
    assert_eq!(first_variant_node.child_count(), 2);

    let discriminant = syntax.child(first_variant, 1).unwrap();
    let discriminant_node = syntax.node(discriminant).unwrap();

    assert_eq!(discriminant_node.kind(), SyntaxNodeKind::EnumDiscriminant);

    let expression = syntax.first_child(discriminant).unwrap();

    assert_eq!(
        syntax.node(expression).unwrap().kind(),
        SyntaxNodeKind::IntegerLiteral
    );
}

#[test]
fn parse_enum_variant_with_binary_discriminant_expression() {
    let source = source(
        "enum(i64) TextureType {
            Float32(1 << 32),
            Float64,
        }",
    );

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();

    let enum_item = syntax.first_child(root).unwrap();

    let variants = syntax
        .first_child_of_kind(enum_item, SyntaxNodeKind::EnumVariantList)
        .unwrap();

    let first_variant = syntax.child(variants, 0).unwrap();

    let discriminant = syntax
        .first_child_of_kind(first_variant, SyntaxNodeKind::EnumDiscriminant)
        .unwrap();

    let expression = syntax.first_child(discriminant).unwrap();
    let expression_node = syntax.node(expression).unwrap();

    assert_eq!(expression_node.kind(), SyntaxNodeKind::BinaryExpression);

    let operator = syntax
        .first_child_of_kind(expression, SyntaxNodeKind::BinaryOperator)
        .unwrap();

    let operator_node = syntax.node(operator).unwrap();

    assert_eq!(
        operator_node.binary_operator(),
        Some(BinaryOperatorKind::ShiftLeft)
    );
}

#[test]
fn parse_choice_payload_item_decorator() {
    let source = source(
        r#"
        choice Asset {
            Texture(@path [u8]),
            Image(@path [u8], @min(1) i32, @min(1) i32),
        }
        "#,
    );

    let result = parse(&source);

    assert!(!result.has_errors());

    let graph = result.graph();
    let syntax = graph.syntax();
    let root = syntax.root().unwrap();

    assert!(find_first_of_kind(syntax, root, SyntaxNodeKind::ChoicePayloadItem).is_some());
}

#[test]
fn parse_choice_payload_single_item() {
    let source = source(
        r#"
        choice Option<T> {
            Some(T),
            None,
        }
        "#,
    );

    let result = parse(&source);

    assert!(!result.has_errors());
}
