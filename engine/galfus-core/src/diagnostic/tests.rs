use super::*;
use crate::{SourceId, Span};

fn span() -> Span {
    Span::new(SourceId::new(0), 3, 8)
}

enum ErrorCode {
    DemoE,
    DemoW,
}

impl DiagnosticCodeKind for ErrorCode {
    fn as_code(&self) -> &'static str {
        match self {
            Self::DemoE => "E0001",
            Self::DemoW => "W0001",
        }
    }

    fn as_message(&self) -> &'static str {
        match self {
            Self::DemoE => "unexpected token",
            Self::DemoW => "unused variable",
        }
    }
}

#[test]
fn diagnostic_code_stores_code_text() {
    let code = DiagnosticCode::new("E0001");

    assert_eq!(code.as_str(), "E0001");
}

#[test]
fn diagnostic_new_stores_all_fields() {
    let diagnostic = Diagnostic::new(
        DiagnosticSeverity::Error,
        DiagnosticCode::new("E0001"),
        "unexpected token",
        span(),
    );

    assert_eq!(diagnostic.severity(), DiagnosticSeverity::Error);
    assert_eq!(diagnostic.code().as_str(), "E0001");
    assert_eq!(diagnostic.message(), "unexpected token");
    assert_eq!(diagnostic.span(), span());
    assert!(diagnostic.is_error());
}

#[test]
fn diagnostic_error_creates_error_diagnostic() {
    let diagnostic = Diagnostic::error(ErrorCode::DemoE, span());

    assert_eq!(diagnostic.severity(), DiagnosticSeverity::Error);
    assert_eq!(diagnostic.code().as_str(), "E0001");
    assert_eq!(diagnostic.message(), "unexpected token");
    assert_eq!(diagnostic.span(), span());
    assert!(diagnostic.is_error());
}

#[test]
fn diagnostic_warning_creates_warning_diagnostic() {
    let diagnostic = Diagnostic::warning(ErrorCode::DemoW, span());

    assert_eq!(diagnostic.severity(), DiagnosticSeverity::Warning);
    assert_eq!(diagnostic.code().as_str(), "W0001");
    assert_eq!(diagnostic.message(), "unused variable");
    assert_eq!(diagnostic.span(), span());
    assert!(!diagnostic.is_error());
}

#[test]
fn diagnostic_bag_starts_empty() {
    let bag = DiagnosticBag::new();

    assert!(bag.is_empty());
    assert_eq!(bag.len(), 0);
    assert!(!bag.has_errors());
}

#[test]
fn diagnostic_bag_push_adds_diagnostic() {
    let mut bag = DiagnosticBag::new();

    bag.push(Diagnostic::warning(ErrorCode::DemoW, span()));

    assert!(!bag.is_empty());
    assert_eq!(bag.len(), 1);
    assert!(!bag.has_errors());
}

#[test]
fn diagnostic_bag_has_errors_detects_error_diagnostics() {
    let mut bag = DiagnosticBag::new();

    bag.push(Diagnostic::warning(ErrorCode::DemoW, span()));
    bag.push(Diagnostic::error(ErrorCode::DemoE, span()));

    assert_eq!(bag.len(), 2);
    assert!(bag.has_errors());
}

#[test]
fn diagnostic_bag_iterates_over_diagnostics() {
    let mut bag = DiagnosticBag::new();

    bag.push(Diagnostic::warning(ErrorCode::DemoW, span()));
    bag.push(Diagnostic::error(ErrorCode::DemoE, span()));

    let codes: Vec<&str> = bag
        .iter()
        .map(|diagnostic| diagnostic.code().as_str())
        .collect();

    assert_eq!(codes, vec!["W0001", "E0001"]);
}

#[test]
fn diagnostic_bag_into_vec_returns_inner_diagnostics() {
    let mut bag = DiagnosticBag::new();

    bag.push(Diagnostic::warning(ErrorCode::DemoW, span()));
    bag.push(Diagnostic::error(ErrorCode::DemoE, span()));

    let diagnostics = bag.into_vec();

    assert_eq!(diagnostics.len(), 2);
    assert_eq!(diagnostics[0].code().as_str(), "W0001");
    assert_eq!(diagnostics[1].code().as_str(), "E0001");
}

#[test]
fn diagnostic_error_with_message_overrides_default_message() {
    let diagnostic =
        Diagnostic::error_with_message(ErrorCode::DemoE, "custom parser message", span());

    assert_eq!(diagnostic.severity(), DiagnosticSeverity::Error);
    assert_eq!(diagnostic.code().as_str(), "E0001");
    assert_eq!(diagnostic.message(), "custom parser message");
    assert_eq!(diagnostic.span(), span());
    assert!(diagnostic.is_error());
}
