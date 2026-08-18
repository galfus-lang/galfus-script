use galfus_core::{Diagnostic, RowCol, SourceFile};
use lsp_types::{
    Diagnostic as LspDiagnostic, DiagnosticSeverity as LspDiagnosticSeverity, Position, Range,
};

pub fn format_diagnostic_rich(diagnostic: &Diagnostic, source: &SourceFile) -> String {
    let start_offset = diagnostic.span().start();
    let start_row_col = source
        .row_col(start_offset)
        .unwrap_or(RowCol { row: 1, column: 1 });

    let mut message = diagnostic.message().to_string();
    message.push_str("\n\n");
    message.push_str(&format!(
        "  --> {}:{}:{}\n",
        source.name(),
        start_row_col.row,
        start_row_col.column
    ));
    message.push_str("   |\n");

    let line_text = source
        .text()
        .lines()
        .nth(start_row_col.row.saturating_sub(1))
        .unwrap_or("");
    let line_num_str = start_row_col.row.to_string();

    message.push_str(&format!("{:>2} | {}\n", line_num_str, line_text));

    let indent = " ".repeat(start_row_col.column.saturating_sub(1));
    let span_len = diagnostic.span().len().max(1);
    let pointer = "^".repeat(span_len);

    let line_padding = " ".repeat(line_num_str.len().max(2));
    message.push_str(&format!("{} | {}{}", line_padding, indent, pointer));

    message
}

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

    let message = format_diagnostic_rich(diagnostic, source);

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
