use super::*;

#[test]
fn proxy_module_rejects_invalid_items() {
    let (_source, _graph, result, _string_table) = check_proxy_source(
        r#"
var x = 10
const Y: i32 = 20

enum MyEnum {
    A
}

choice MyChoice {
    A
}
"#,
    );

    assert!(result.has_errors());
    assert_eq!(result.diagnostics().len(), 4);

    for diagnostic in result.diagnostics().iter() {
        assert_eq!(
            diagnostic.code().as_str(),
            TypeDiagnosticCode::ProxyModuleInvalidItem.as_code()
        );
    }
}

#[test]
fn proxy_module_rejects_function_body() {
    let (_source, _graph, result, _string_table) = check_proxy_source(
        r#"
export fn with_body(): null {
    var x = 10
}

export fn without_body(): null
"#,
    );

    assert!(result.has_errors());
    assert_eq!(result.diagnostics().len(), 1);
    assert_eq!(
        result.diagnostics().iter().next().unwrap().code().as_str(),
        TypeDiagnosticCode::ProxyModuleInvalidItem.as_code()
    );
}

#[test]
fn proxy_module_accepts_valid_items() {
    let (_source, _graph, result, _string_table) = check_proxy_source(
        r#"
struct Window {}

type Point = i32

export fn open(w: Window): null

export struct MyProxy {}
"#,
    );

    assert!(!result.has_errors(), "{:?}", result.diagnostics());
}

#[test]
fn opaque_handle_not_exportable_as_value() {
    let (_source, _graph, result, _string_table) = check_proxy_source(
        r#"
struct Window {}

export const w = Window
"#,
    );

    assert!(result.has_errors());
    assert!(result.diagnostics().iter().any(
        |d| d.code().as_str() == TypeDiagnosticCode::OpaqueHandleNotExportableAsValue.as_code()
    ));
}
