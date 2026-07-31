use super::*;
use galfus_core::{SourceId, Span};
use smallvec::smallvec;

#[test]
fn syntax_layer_starts_empty() {
    let syntax = SyntaxLayer::new();

    assert!(syntax.root().is_none());
    assert!(syntax.tokens().is_empty());
    assert!(syntax.nodes().is_empty());
    assert!(syntax.is_empty());
    assert_eq!(syntax.len(), 0);
}

#[test]
fn syntax_layer_adds_node() {
    let span = Span::new(SourceId::new(0), 0, 4);

    let mut syntax = SyntaxLayer::new();

    let id = syntax.add_node(SyntaxNodeKind::Identifier, span, Vec::new());

    assert_eq!(id, NodeId::new(0));
    assert_eq!(syntax.len(), 1);

    let node = syntax.node(id).unwrap();

    assert_eq!(node.kind(), SyntaxNodeKind::Identifier);
    assert_eq!(node.span(), span);
    assert!(node.children().is_empty());
}

#[test]
fn syntax_layer_stores_root() {
    let span = Span::new(SourceId::new(0), 0, 0);

    let mut syntax = SyntaxLayer::new();

    let root = syntax.add_node(SyntaxNodeKind::SourceFile, span, Vec::new());

    syntax.set_root(root);

    assert_eq!(syntax.root(), Some(root));
}

#[test]
fn module_ast_has_syntax_layer() {
    let source_id = SourceId::new(0);
    let ast = ModuleAst::new(source_id);

    assert_eq!(ast.source_id(), source_id);
    assert_eq!(ast.phase(), AstPhase::Parsed);
    assert!(ast.syntax().is_empty());
    assert!(!ast.has_errors());
}

#[test]
fn syntax_node_exposes_child_helpers() {
    let node = SyntaxNode::new(
        SyntaxNodeKind::SourceFile,
        Span::new(SourceId::new(0), 0, 10),
        smallvec![NodeId::new(1), NodeId::new(2)],
    );

    assert_eq!(node.child_count(), 2);
    assert_eq!(node.first_child(), Some(NodeId::new(1)));
    assert_eq!(node.last_child(), Some(NodeId::new(2)));
    assert_eq!(node.child(0), Some(NodeId::new(1)));
    assert_eq!(node.child(1), Some(NodeId::new(2)));
    assert_eq!(node.child(2), None);
    assert!(node.is(SyntaxNodeKind::SourceFile));
}

#[test]
fn syntax_layer_exposes_child_navigation_helpers() {
    let mut syntax = SyntaxLayer::new();

    let first = syntax.add_node(
        SyntaxNodeKind::Identifier,
        Span::new(SourceId::new(0), 0, 1),
        vec![],
    );

    let second = syntax.add_node(
        SyntaxNodeKind::StringLiteral,
        Span::new(SourceId::new(0), 2, 5),
        vec![],
    );

    let parent = syntax.add_node(
        SyntaxNodeKind::SourceFile,
        Span::new(SourceId::new(0), 0, 5),
        vec![first, second],
    );

    assert_eq!(syntax.child(parent, 0), Some(first));
    assert_eq!(syntax.child(parent, 1), Some(second));
    assert_eq!(syntax.child(parent, 2), None);

    assert_eq!(syntax.first_child(parent), Some(first));
    assert_eq!(syntax.last_child(parent), Some(second));
    assert_eq!(syntax.child_count(parent), Some(2));

    assert_eq!(
        syntax.first_child_of_kind(parent, SyntaxNodeKind::StringLiteral),
        Some(second)
    );

    let identifiers: Vec<_> = syntax
        .children_of_kind(parent, SyntaxNodeKind::Identifier)
        .collect();

    assert_eq!(identifiers, vec![first]);
}

#[test]
fn syntax_node_kind_classifies_major_groups() {
    assert!(SyntaxNodeKind::FunctionItem.is_item());
    assert!(SyntaxNodeKind::StructItem.is_item());
    assert!(SyntaxNodeKind::VarItem.is_item());
    assert!(SyntaxNodeKind::ConstItem.is_item());

    assert!(SyntaxNodeKind::ReturnStatement.is_statement());
    assert!(SyntaxNodeKind::VarStatement.is_statement());

    assert!(SyntaxNodeKind::CallExpression.is_expression());
    assert!(SyntaxNodeKind::NameExpression.is_expression());
    assert!(SyntaxNodeKind::StringLiteral.is_expression());

    assert!(SyntaxNodeKind::NamedType.is_type());
    assert!(SyntaxNodeKind::Path.is_type());
    assert!(SyntaxNodeKind::UnionType.is_type());

    assert!(SyntaxNodeKind::StringLiteral.is_literal());
    assert!(SyntaxNodeKind::BinaryOperator.is_operator());
    assert!(SyntaxNodeKind::ParameterList.is_list());

    assert!(!SyntaxNodeKind::FunctionItem.is_statement());
    assert!(!SyntaxNodeKind::VarStatement.is_item());
    assert!(!SyntaxNodeKind::Identifier.is_expression());
}

#[test]
fn binary_operator_kind_reports_precedence_and_associativity() {
    assert_eq!(BinaryOperatorKind::Power.precedence(), 80);
    assert_eq!(
        BinaryOperatorKind::Power.associativity(),
        BinaryAssociativity::Right
    );

    assert_eq!(BinaryOperatorKind::Multiply.precedence(), 70);
    assert_eq!(
        BinaryOperatorKind::Multiply.associativity(),
        BinaryAssociativity::Left
    );

    assert_eq!(BinaryOperatorKind::NullFallback.precedence(), 10);
    assert_eq!(
        BinaryOperatorKind::NullFallback.associativity(),
        BinaryAssociativity::Right
    );
}

#[test]
fn operator_kinds_are_created_from_tokens() {
    assert_eq!(
        UnaryOperatorKind::from_token(&TokenKind::Bang),
        Some(UnaryOperatorKind::Not)
    );

    assert_eq!(
        BinaryOperatorKind::from_token(&TokenKind::Plus),
        Some(BinaryOperatorKind::Add)
    );

    assert_eq!(
        AssignmentOperatorKind::from_token(&TokenKind::PlusEqual),
        Some(AssignmentOperatorKind::AddAssign)
    );
}
