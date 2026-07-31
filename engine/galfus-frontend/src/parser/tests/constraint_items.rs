use super::*;

#[test]
fn parse_constraint_with_field() {
    let source = source("constraint Identifiable {\n  id: i64,\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let constraint = syntax.node(root).unwrap().first_child().unwrap();
    let constraint_node = syntax.node(constraint).unwrap();

    assert_eq!(constraint_node.kind(), SyntaxNodeKind::ConstraintItem);
    assert_eq!(constraint_node.child_count(), 2);

    let name = constraint_node.first_child().unwrap();
    let members = constraint_node.child(1).unwrap();

    assert_eq!(
        source.slice(syntax.node(name).unwrap().span()),
        Some("Identifiable")
    );

    let members_node = syntax.node(members).unwrap();

    assert_eq!(members_node.kind(), SyntaxNodeKind::ConstraintMemberList);

    assert_eq!(members_node.child_count(), 1);

    let field = members_node.first_child().unwrap();
    let field_node = syntax.node(field).unwrap();

    assert_eq!(field_node.kind(), SyntaxNodeKind::ConstraintField);
    assert_eq!(source.slice(field_node.span()), Some("id: i64"));

    let field_name = field_node.first_child().unwrap();
    let field_type = field_node.child(1).unwrap();

    assert_eq!(
        source.slice(syntax.node(field_name).unwrap().span()),
        Some("id")
    );

    assert_eq!(
        syntax.node(field_type).unwrap().kind(),
        SyntaxNodeKind::NamedType
    );
}

#[test]
fn parse_constraint_with_function_signature() {
    let source = source("constraint Stringable {\n  fn toString(self): [i8]\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let constraint = syntax.node(root).unwrap().first_child().unwrap();
    let constraint_node = syntax.node(constraint).unwrap();

    assert_eq!(constraint_node.kind(), SyntaxNodeKind::ConstraintItem);

    let members = constraint_node.child(1).unwrap();
    let member = syntax.node(members).unwrap().first_child().unwrap();
    let signature_node = syntax.node(member).unwrap();

    assert_eq!(
        signature_node.kind(),
        SyntaxNodeKind::ConstraintFunctionSignature
    );

    assert_eq!(
        source.slice(signature_node.span()),
        Some("fn toString(self): [i8]")
    );

    assert_eq!(signature_node.child_count(), 3);

    let name = signature_node.first_child().unwrap();
    let parameters = signature_node.child(1).unwrap();
    let return_type = signature_node.child(2).unwrap();

    assert_eq!(
        source.slice(syntax.node(name).unwrap().span()),
        Some("toString")
    );

    assert_eq!(
        syntax.node(parameters).unwrap().kind(),
        SyntaxNodeKind::ParameterList
    );

    assert_eq!(
        syntax.node(return_type).unwrap().kind(),
        SyntaxNodeKind::ArrayType
    );
}

#[test]
fn parse_generic_constraint_with_constrained_parameter() {
    let source = source("constraint ParseInteger<T: int> {\n  fn toInt(self): T\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let constraint = syntax.node(root).unwrap().first_child().unwrap();
    let constraint_node = syntax.node(constraint).unwrap();

    assert_eq!(constraint_node.kind(), SyntaxNodeKind::ConstraintItem);
    assert_eq!(constraint_node.child_count(), 3);

    let name = constraint_node.first_child().unwrap();
    let generics = constraint_node.child(1).unwrap();
    let members = constraint_node.child(2).unwrap();

    assert_eq!(
        source.slice(syntax.node(name).unwrap().span()),
        Some("ParseInteger")
    );

    let generics_node = syntax.node(generics).unwrap();

    assert_eq!(generics_node.kind(), SyntaxNodeKind::GenericParameterList);

    assert_eq!(source.slice(generics_node.span()), Some("<T: int>"));

    let generic_parameter = generics_node.first_child().unwrap();
    let generic_parameter_node = syntax.node(generic_parameter).unwrap();

    assert_eq!(
        generic_parameter_node.kind(),
        SyntaxNodeKind::GenericParameter
    );

    assert_eq!(generic_parameter_node.child_count(), 2);

    let member = syntax.node(members).unwrap().first_child().unwrap();

    assert_eq!(
        syntax.node(member).unwrap().kind(),
        SyntaxNodeKind::ConstraintFunctionSignature
    );
}

#[test]
fn parse_constraint_with_field_and_function() {
    let source = source("constraint Entity {\n  id: i64,\n  fn toString(self): [i8]\n}");

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();

    let root = syntax.root().unwrap();
    let constraint = syntax.node(root).unwrap().first_child().unwrap();
    let constraint_node = syntax.node(constraint).unwrap();

    let members = constraint_node.child(1).unwrap();
    let members_node = syntax.node(members).unwrap();

    assert_eq!(members_node.child_count(), 2);

    let field = members_node.first_child().unwrap();
    let signature = members_node.child(1).unwrap();

    assert_eq!(
        syntax.node(field).unwrap().kind(),
        SyntaxNodeKind::ConstraintField
    );

    assert_eq!(
        syntax.node(signature).unwrap().kind(),
        SyntaxNodeKind::ConstraintFunctionSignature
    );
}
