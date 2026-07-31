use super::*;
use galfus_core::{SourceFile, SourceId};
use galfus_frontend::{check_declaration_types, parse, resolve};

fn assert_builtin_checks(name: &str, source: &str) {
    assert!(!source.is_empty());

    let source_file = SourceFile::new(SourceId::new(0), name.to_string(), source.to_string());
    let parse_result = parse(&source_file);
    assert!(
        !parse_result.has_errors(),
        "{name} parse errors: {:?}",
        parse_result.diagnostics()
    );

    let resolve_result = resolve(&source_file, parse_result.into_graph());
    assert!(
        !resolve_result.has_errors(),
        "{name} resolve errors: {:?}",
        resolve_result.diagnostics()
    );

    let graph = resolve_result.into_graph();
    let type_result = check_declaration_types(&source_file, &graph);
    assert!(
        !type_result.has_errors(),
        "{name} type errors: {:?}",
        type_result.diagnostics()
    );
}

#[test]
fn test_std_io_source_checks() {
    assert_builtin_checks("std/io", STD_IO_SOURCE);
    assert!(STD_IO_SOURCE.contains("print"));
    assert!(STD_IO_SOURCE.contains("read"));
}

#[test]
fn test_text_source_checks() {
    assert_builtin_checks("text", TEXT_SOURCE);
    assert!(TEXT_SOURCE.contains("length"));
    assert!(TEXT_SOURCE.contains("concat"));
}

#[test]
fn test_format_source_checks() {
    assert_builtin_checks("format", FORMAT_SOURCE);
    assert!(FORMAT_SOURCE.contains("stringify"));
    assert!(FORMAT_SOURCE.contains("parse"));
    assert!(FORMAT_SOURCE.contains("Result"));
}

#[test]
fn test_format_ansi_source_checks() {
    assert_builtin_checks("format/ansi", FORMAT_ANSI_SOURCE);
    assert!(FORMAT_ANSI_SOURCE.contains("Style"));
    assert!(FORMAT_ANSI_SOURCE.contains("apply"));
    assert!(FORMAT_ANSI_SOURCE.contains("red"));
}

#[test]
fn test_thread_source_checks() {
    assert_builtin_checks("std/thread", THREAD_SOURCE);
    assert!(THREAD_SOURCE.contains("isRunning"));
    assert!(THREAD_SOURCE.contains("isExited"));
    assert!(THREAD_SOURCE.contains("exitReason"));
    assert!(THREAD_SOURCE.contains("fn Thread::send(self, data: [u8]): bool"));
}
