use galfus_core::{Diagnostic, RowCol, SourceFile};
use lsp_types::{
    Diagnostic as LspDiagnostic, DiagnosticSeverity as LspDiagnosticSeverity, Position, Range,
};

pub fn convert_diagnostic(diagnostic: &Diagnostic, source: &SourceFile) -> LspDiagnostic {
    let start_offset = diagnostic.span().start();
    let end_offset = diagnostic.span().end();

    let start_row_col = source
        .row_col(start_offset)
        .unwrap_or(RowCol { row: 1, column: 1 });

    let end_row_col = source.row_col(end_offset).unwrap_or(start_row_col);

    let severity = match diagnostic.severity() {
        galfus_core::DiagnosticSeverity::Error => Some(LspDiagnosticSeverity::ERROR),
        galfus_core::DiagnosticSeverity::Warning => Some(LspDiagnosticSeverity::WARNING),
        galfus_core::DiagnosticSeverity::Info => Some(LspDiagnosticSeverity::INFORMATION),
        galfus_core::DiagnosticSeverity::Hint => Some(LspDiagnosticSeverity::HINT),
    };

    let message = diagnostic.message().to_string();

    LspDiagnostic {
        range: Range {
            start: Position {
                line: start_row_col.row.saturating_sub(1) as u32,
                character: start_row_col.column.saturating_sub(1) as u32,
            },
            end: Position {
                line: end_row_col.row.saturating_sub(1) as u32,
                character: end_row_col.column.saturating_sub(1) as u32,
            },
        },
        severity,
        code: Some(lsp_types::NumberOrString::String(
            diagnostic.code().as_str().to_string(),
        )),
        code_description: None,
        source: Some("galfus".to_string()),
        message,
        related_information: None,
        tags: None,
        data: None,
    }
}
