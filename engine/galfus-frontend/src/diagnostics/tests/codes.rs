use crate::{
    LexicalDiagnosticCode, ParserDiagnosticCode, ResolverDiagnosticCode, TokenTreeDiagnosticCode,
};
use galfus_core::DiagnosticCodeKind;

#[test]
fn diagnostic_areas_keep_stable_prefixes() {
    assert_eq!(LexicalDiagnosticCode::UnknownCharacter.as_code(), "L0004");
    assert_eq!(ParserDiagnosticCode::ExpectedToken.as_code(), "P0001");
    assert_eq!(ResolverDiagnosticCode::DuplicateSymbol.as_code(), "S0001");
    assert_eq!(ResolverDiagnosticCode::UnresolvedName.as_code(), "S0002");
    assert_eq!(ResolverDiagnosticCode::UnresolvedType.as_code(), "S0003");
    assert_eq!(
        ResolverDiagnosticCode::InvalidFunctionAnchor.as_code(),
        "S0004"
    );
    assert_eq!(
        TokenTreeDiagnosticCode::UnclosedDelimiter.as_code(),
        "B0001"
    );
    assert_eq!(
        TokenTreeDiagnosticCode::UnexpectedClosingDelimiter.as_code(),
        "B0002"
    );
}
