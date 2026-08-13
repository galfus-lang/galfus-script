use dialoguer::console::style;
use galfus_core::{DiagnosticBag, SourceFile};
use galfus_workspace::source_store::SourceStore;

pub fn print_diagnostics(diagnostics: &DiagnosticBag, store: &SourceStore) {
    for diagnostic in diagnostics.iter() {
        let severity = diagnostic.severity();
        let code = diagnostic.code().as_str();
        let message = diagnostic.message();
        let span = diagnostic.span();

        let header = match severity {
            galfus_core::DiagnosticSeverity::Error => {
                style(format!("error[{}]", code)).red().bold()
            }
            galfus_core::DiagnosticSeverity::Warning => {
                style(format!("warning[{}]", code)).yellow().bold()
            }
            galfus_core::DiagnosticSeverity::Info => style(format!("info[{}]", code)).blue().bold(),
            galfus_core::DiagnosticSeverity::Hint => style(format!("hint[{}]", code)).cyan().bold(),
        };

        // Format: error[CODE]: message
        println!("{}: {}", header, style(message).bold());

        // Try to fetch source file
        if let Some(entry) = store.iter().find(|e| e.source_id == span.source_id()) {
            let text = String::from_utf8_lossy(&entry.bytes).to_string();
            let source_file =
                SourceFile::new(entry.source_id, entry.path.as_str().to_string(), text);

            if let Some(row_col) = source_file.row_col(span.start()) {
                println!(
                    "  {} {}:{}:{}",
                    style("-->").blue().bold(),
                    entry.path.as_str(),
                    row_col.row,
                    row_col.column
                );

                println!("   {}", style("|").blue().bold());

                let line_text = source_file
                    .text()
                    .lines()
                    .nth(row_col.row.saturating_sub(1))
                    .unwrap_or("");
                let line_num_str = row_col.row.to_string();

                println!(
                    "{:>2} {} {}",
                    style(&line_num_str).blue().bold(),
                    style("|").blue().bold(),
                    line_text
                );

                let indent = " ".repeat(row_col.column.saturating_sub(1));
                let span_len = span.len().max(1);
                let pointer = "^".repeat(span_len);

                let pointer_styled = match severity {
                    galfus_core::DiagnosticSeverity::Error => style(pointer).red().bold(),
                    galfus_core::DiagnosticSeverity::Warning => style(pointer).yellow().bold(),
                    galfus_core::DiagnosticSeverity::Info => style(pointer).blue().bold(),
                    galfus_core::DiagnosticSeverity::Hint => style(pointer).cyan().bold(),
                };

                println!(
                    "   {} {}{}",
                    style("|").blue().bold(),
                    indent,
                    pointer_styled
                );
                println!();
            }
        } else {
            println!();
        }
    }
}
