use crate::workspace::Workspace;
use serde_json::{Value, json};

#[test]
fn test_lsp_initialize() {
    let mut workspace = Workspace::new();
    let req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {}
    })
    .to_string();

    let responses = workspace.handle_lsp_message(&req);
    assert_eq!(responses.len(), 1);

    let response_val: Value = serde_json::from_str(&responses[0]).unwrap();
    assert_eq!(response_val["id"], 1);
    assert!(
        response_val["result"]["capabilities"]["hoverProvider"]
            .as_bool()
            .unwrap()
    );
}

#[test]
fn test_lsp_did_open_and_diagnostics() {
    let mut workspace = Workspace::new();
    let manifest_toml = r#"
[module]
name = "test"
target = "app"
[entry]
path = "src/main.gfs"
"#;
    let manifest: crate::config::WorkspaceManifest = toml::from_str(manifest_toml).unwrap();
    workspace.load_manifest(manifest).unwrap();

    let req = json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": "file:///src/main.gfs",
                "languageId": "galfus",
                "version": 1,
                "text": "fn main(): null { const x = 1" // missing closing brace
            }
        }
    })
    .to_string();

    let responses = workspace.handle_lsp_message(&req);
    // Should emit publishDiagnostics
    assert_eq!(responses.len(), 1);

    let response_val: Value = serde_json::from_str(&responses[0]).unwrap();
    assert_eq!(response_val["method"], "textDocument/publishDiagnostics");
    assert_eq!(response_val["params"]["uri"], "file:///src/main.gfs");
    let diagnostics = response_val["params"]["diagnostics"].as_array().unwrap();
    assert!(
        !diagnostics.is_empty(),
        "Expected diagnostics for syntax error"
    );
}

#[test]
fn test_lsp_hover() {
    let mut workspace = Workspace::new();
    let manifest_toml = r#"
[module]
name = "test"
target = "app"
[entry]
path = "src/main.gfs"
"#;
    let manifest: crate::config::WorkspaceManifest = toml::from_str(manifest_toml).unwrap();
    workspace.load_manifest(manifest).unwrap();

    // First open a valid file
    let open_req = json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": "file:///src/main.gfs",
                "languageId": "galfus",
                "version": 1,
                "text": "fn main(): null { const x = 1 }"
            }
        }
    })
    .to_string();
    let _ = workspace.handle_lsp_message(&open_req);

    // Now request hover
    let hover_req = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "textDocument/hover",
        "params": {
            "textDocument": {
                "uri": "file:///src/main.gfs"
            },
            "position": {
                "line": 0,
                "character": 24 // hovering over 'x' in 'fn main(): null { const x = 1 }'
            }
        }
    })
    .to_string();

    let responses = workspace.handle_lsp_message(&hover_req);
    assert_eq!(responses.len(), 1);

    let response_val: Value = serde_json::from_str(&responses[0]).unwrap();
    assert_eq!(response_val["id"], 2);

    // Hover response might be null if not found, but it should return Some Hover
    // We expect it to not be null because we are over a valid node.
    assert!(
        !response_val["result"].is_null(),
        "Expected Hover response, got null"
    );
}
