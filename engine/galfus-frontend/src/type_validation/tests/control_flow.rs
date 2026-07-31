use super::*;
use crate::type_validation::check_definition_types;

#[test]
fn check_accepts_if_bool_condition() {
    let (_source, _graph, result) = check_source(
        r#"
fn main(): null {
  if true {
    return
  }

  return
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_reports_if_non_bool_condition() {
    let source = source(
        r#"
fn main(): null {
  if 1 {
    return
  }

  return
}
"#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.into_graph();
    let result = check_declaration_types(&source, &graph);
    let result = check_definition_types(&source, &graph, result);

    assert!(result.has_errors());
    assert!(result.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == TypeDiagnosticCode::InvalidConditionType.as_code()
            && diagnostic
                .message()
                .contains("condition must be `bool`, got `i32`")
    }));
}

#[test]
fn check_accepts_break_inside_loop() {
    let (_source, _graph, result) = check_source(
        r#"
fn main(): null {
  loop {
    break
  }

  return
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_continue_inside_loop() {
    let (_source, _graph, result) = check_source(
        r#"
fn main(): null {
  loop {
    continue
  }

  return
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_reports_break_outside_loop() {
    let source = source(
        r#"
fn main(): null {
  break
  return
}
"#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.into_graph();
    let result = check_declaration_types(&source, &graph);
    let result = check_definition_types(&source, &graph, result);

    assert!(result.has_errors());
    assert!(result.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == TypeDiagnosticCode::BreakOutsideLoop.as_code()
            && diagnostic
                .message()
                .contains("`break` can only be used inside a loop")
    }));
}

#[test]
fn check_reports_continue_outside_loop() {
    let source = source(
        r#"
fn main(): null {
  continue
  return
}
"#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());

    let resolve_result = resolve(&source, parse_result.into_graph());
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.into_graph();
    let result = check_declaration_types(&source, &graph);
    let result = check_definition_types(&source, &graph, result);

    assert!(result.has_errors());
    assert!(result.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == TypeDiagnosticCode::ContinueOutsideLoop.as_code()
            && diagnostic
                .message()
                .contains("`continue` can only be used inside a loop")
    }));
}

#[test]
fn check_accepts_named_control_target() {
    let (_source, _graph, result) = check_source(
        r#"
fn main(): null {
  loop(name: outer) {
    loop(name: inner) {
      break outer
    }
  }
  return
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_accepts_named_for_control_target() {
    let (_source, _graph, result) = check_source(
        r#"
fn main(values: [i32]): null {
  for(name: values) value in values {
    continue values
  }
  return
}
"#,
    );

    assert!(!result.has_errors());
}

#[test]
fn check_reports_duplicate_control_target() {
    let source = source(
        r#"
fn main(): null {
  loop(name: outer) {
    loop(name: outer) {
      break outer
    }
  }
  return
}
"#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());
    let resolve_result = resolve(&source, parse_result.into_graph());
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.into_graph();
    let result = check_declaration_types(&source, &graph);
    let result = check_definition_types(&source, &graph, result);
    assert!(result.has_errors());
    assert!(result.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == TypeDiagnosticCode::DuplicateControlTarget.as_code()
            && diagnostic
                .message()
                .contains("duplicate control target name outer")
    }));
}

#[test]
fn check_reports_unresolved_control_target() {
    let source = source(
        r#"
fn main(): null {
  loop(name: outer) {
    break unknown
  }
  return
}
"#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());
    let resolve_result = resolve(&source, parse_result.into_graph());
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.into_graph();
    let result = check_declaration_types(&source, &graph);
    let result = check_definition_types(&source, &graph, result);
    assert!(result.has_errors());
    assert!(result.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == TypeDiagnosticCode::UnresolvedControlTarget.as_code()
            && diagnostic
                .message()
                .contains("unresolved control target unknown")
    }));
}

#[test]
fn check_reports_invalid_metadata_for_loop() {
    let source = source(
        r#"
fn main(): null {
  loop(stamp) {
    break
  }
  return
}
"#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());
    let resolve_result = resolve(&source, parse_result.into_graph());
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.into_graph();
    let result = check_declaration_types(&source, &graph);
    let result = check_definition_types(&source, &graph, result);
    assert!(result.has_errors());
    assert!(result.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == TypeDiagnosticCode::InvalidKeywordMetadata.as_code()
            && diagnostic
                .message()
                .contains("invalid metadata stamp for loop")
    }));
}

#[test]
fn check_reports_decorators_on_stamped_function() {
    let source = source(
        r#"
fn foo(target: fn(): null): fn(): null {
  return target
}

@foo
fn(stamp) stamped_fn(): null {
  return
}
"#,
    );

    let parse_result = parse(&source);
    assert!(!parse_result.has_errors());
    let resolve_result = resolve(&source, parse_result.into_graph());
    assert!(!resolve_result.has_errors());

    let graph = resolve_result.into_graph();
    let result = check_declaration_types(&source, &graph);
    let result = check_definition_types(&source, &graph, result);
    assert!(result.has_errors());
    assert!(result.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == TypeDiagnosticCode::InvalidDecoratorUsage.as_code()
            && diagnostic
                .message()
                .contains("decorators are not allowed on stamped functions")
    }));
}
