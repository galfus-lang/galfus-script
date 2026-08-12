use super::*;
use galfus_core::{DiagnosticBag, DiagnosticCodeKind};

#[test]
fn test_default_limits_are_applied_when_not_specified() {
    let toml = r#"
        [module]
        name = "test"
        version = "1.0.0"
        target = "app"

        [entry]
        path = "main.gfs"
    "#;
    let mut diagnostics = DiagnosticBag::new();
    let config = parse_workspace_config(toml, &mut diagnostics);
    assert!(
        !diagnostics.has_errors(),
        "Diagnostics: {:?}",
        diagnostics.into_vec()
    );
    let config = config.unwrap();

    let default_limits = LimitsMetadata::default();
    assert_eq!(
        config.limits().max_heap_objects,
        default_limits.max_heap_objects
    );
    assert_eq!(
        config.limits().max_heap_bytes,
        default_limits.max_heap_bytes
    );
    assert_eq!(config.limits().max_threads, default_limits.max_threads);
}

#[test]
fn test_overrides_are_applied() {
    let toml = r#"
        [module]
        name = "test"
        version = "1.0.0"
        target = "app"

        [entry]
        path = "main.gfs"

        [limits]
        max_heap_objects = 999
        max_heap_bytes = 888
    "#;
    let mut diagnostics = DiagnosticBag::new();
    let config = parse_workspace_config(toml, &mut diagnostics);
    assert!(
        !diagnostics.has_errors(),
        "Diagnostics: {:?}",
        diagnostics.into_vec()
    );
    let config = config.unwrap();

    assert_eq!(config.limits().max_heap_objects, 999);
    assert_eq!(config.limits().max_heap_bytes, 888);
    // Other limits remain default
    assert_eq!(
        config.limits().max_threads,
        LimitsMetadata::default().max_threads
    );
}

#[test]
fn test_zero_limit_is_rejected() {
    let toml = r#"
        [module]
        name = "test"
        version = "1.0.0"
        target = "app"

        [entry]
        path = "main.gfs"

        [limits]
        max_threads = 0
    "#;
    let mut diagnostics = DiagnosticBag::new();
    let _ = parse_workspace_config(toml, &mut diagnostics);
    assert!(diagnostics.has_errors());
    let errors = diagnostics.into_vec();
    assert_eq!(
        errors[0].code().as_str(),
        WorkspaceDiagnosticCode::InvalidLimit.as_code()
    );
}

#[test]
fn test_negative_limit_is_rejected_as_invalid_config() {
    let toml = r#"
        [module]
        name = "test"
        version = "1.0.0"
        target = "app"

        [entry]
        path = "main.gfs"

        [limits]
        max_threads = -1
    "#;
    let mut diagnostics = DiagnosticBag::new();
    let _ = parse_workspace_config(toml, &mut diagnostics);
    assert!(diagnostics.has_errors());
    let errors = diagnostics.into_vec();
    assert_eq!(
        errors[0].code().as_str(),
        WorkspaceDiagnosticCode::InvalidConfig.as_code()
    );
    assert!(
        errors[0].message().contains("invalid value: integer `-1`")
            || errors[0].message().contains("invalid type: integer")
    );
}

#[test]
fn test_unknown_key_is_rejected_as_invalid_config() {
    let toml = r#"
        [module]
        name = "test"
        version = "1.0.0"
        target = "app"

        [entry]
        path = "main.gfs"

        [limits]
        unknown_limit = 100
    "#;
    let mut diagnostics = DiagnosticBag::new();
    let _ = parse_workspace_config(toml, &mut diagnostics);
    assert!(diagnostics.has_errors());
    let errors = diagnostics.into_vec();
    assert_eq!(
        errors[0].code().as_str(),
        WorkspaceDiagnosticCode::InvalidConfig.as_code()
    );
    assert!(
        errors[0]
            .message()
            .contains("unknown field `unknown_limit`")
    );
}
