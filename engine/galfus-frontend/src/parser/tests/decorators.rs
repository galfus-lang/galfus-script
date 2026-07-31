use super::*;

#[test]
fn parse_struct_decorator() {
    let source = source(
        "@frozen
        struct User {
            name: [i8],
        }",
    );

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();

    let item = syntax.first_child(root).unwrap();
    let item_node = syntax.node(item).unwrap();

    assert_eq!(item_node.kind(), SyntaxNodeKind::StructItem);

    let decorators = syntax
        .first_child_of_kind(item, SyntaxNodeKind::DecoratorList)
        .unwrap();

    assert_eq!(syntax.node(decorators).unwrap().child_count(), 1);

    let decorator = syntax.first_child(decorators).unwrap();

    assert_eq!(
        syntax.node(decorator).unwrap().kind(),
        SyntaxNodeKind::Decorator
    );
}

#[test]
fn parse_function_decorator() {
    let source = source(
        "@log
        fn saveUser(user: User): bool {
            return true
        }",
    );

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();

    let item = syntax.first_child(root).unwrap();

    assert_eq!(
        syntax.node(item).unwrap().kind(),
        SyntaxNodeKind::FunctionItem
    );

    let decorators = syntax
        .first_child_of_kind(item, SyntaxNodeKind::DecoratorList)
        .unwrap();

    assert_eq!(syntax.node(decorators).unwrap().child_count(), 1);
}

#[test]
fn parse_decorator_with_arguments() {
    let source = source(
        "@min(0)
        fn run(): null {
            return
        }",
    );

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();

    let item = syntax.first_child(root).unwrap();
    let decorators = syntax
        .first_child_of_kind(item, SyntaxNodeKind::DecoratorList)
        .unwrap();

    let decorator = syntax.first_child(decorators).unwrap();
    let target = syntax.first_child(decorator).unwrap();

    assert_eq!(
        syntax.node(target).unwrap().kind(),
        SyntaxNodeKind::CallExpression
    );
}

#[test]
fn parse_decorator_with_path_target() {
    let source = source(
        "@string::trim
        fn run(): null {
            return
        }",
    );

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();

    let item = syntax.first_child(root).unwrap();
    let decorators = syntax
        .first_child_of_kind(item, SyntaxNodeKind::DecoratorList)
        .unwrap();

    let decorator = syntax.first_child(decorators).unwrap();
    let target = syntax.first_child(decorator).unwrap();

    assert_eq!(
        syntax.node(target).unwrap().kind(),
        SyntaxNodeKind::PathExpression
    );
}

#[test]
fn parse_struct_field_decorator() {
    let source = source(
        "struct User {
            @min(0)
            age: i32,
        }",
    );

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();

    let item = syntax.first_child(root).unwrap();
    let fields = syntax
        .first_child_of_kind(item, SyntaxNodeKind::StructFieldList)
        .unwrap();

    let field = syntax.first_child(fields).unwrap();

    assert_eq!(
        syntax.node(field).unwrap().kind(),
        SyntaxNodeKind::StructField
    );

    let decorators = syntax
        .first_child_of_kind(field, SyntaxNodeKind::DecoratorList)
        .unwrap();

    assert_eq!(syntax.node(decorators).unwrap().child_count(), 1);
}

#[test]
fn parse_parameter_decorator() {
    let source = source(
        "fn createUser(
            @string::trim name: [i8],
            @min(0) age: i32,
        ): User {
            return new(User) {
                name,
                age,
            }
        }",
    );

    let result = parse(&source);

    assert!(!result.has_errors());

    let syntax = result.graph().syntax();
    let root = syntax.root().unwrap();

    let item = syntax.first_child(root).unwrap();
    let parameters = syntax
        .first_child_of_kind(item, SyntaxNodeKind::ParameterList)
        .unwrap();

    let first_parameter = syntax.child(parameters, 0).unwrap();
    let second_parameter = syntax.child(parameters, 1).unwrap();

    assert!(
        syntax
            .first_child_of_kind(first_parameter, SyntaxNodeKind::DecoratorList)
            .is_some()
    );

    assert!(
        syntax
            .first_child_of_kind(second_parameter, SyntaxNodeKind::DecoratorList)
            .is_some()
    );
}

#[test]
fn parse_rejects_decorator_on_var_item() {
    let source = source("@memo var value = 1");

    let result = parse(&source);

    assert!(result.has_errors());
}

#[test]
fn parse_rejects_decorator_on_enum_item() {
    let source = source(
        "@flags
        enum State {
            Off,
            On,
        }",
    );

    let result = parse(&source);

    assert!(result.has_errors());
}

#[test]
fn parse_rejects_decorator_on_statement() {
    let source = source(
        "fn main(): null {
            @memo
            var value = 1
            return
        }",
    );

    let result = parse(&source);

    assert!(result.has_errors());
}
