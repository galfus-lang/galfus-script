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
    assert!(
        response_val["result"]["capabilities"]["inlayHintProvider"]
            .as_bool()
            .unwrap()
    );
}

#[test]
fn lsp_inlay_hints_show_unannotated_binding_types_after_the_name() {
    let mut workspace = Workspace::new();
    let manifest: crate::config::WorkspaceManifest = toml::from_str(
        r#"
[module]
name = "test"
target = "app"
[entry]
path = "src/main.gfs"
"#,
    )
    .unwrap();
    workspace.load_manifest(manifest).unwrap();

    let open_request = json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": "file:///src/main.gfs",
                "languageId": "galfus",
                "version": 1,
                "text": "fn main(): null {\n  const count = 0\n  var enabled = true\n  const explicit: i64 = 42\n}"
            }
        }
    })
    .to_string();
    let _ = workspace.handle_lsp_message(&open_request);

    let request = json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "textDocument/inlayHint",
        "params": {
            "textDocument": { "uri": "file:///src/main.gfs" },
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": { "line": 4, "character": 0 }
            }
        }
    })
    .to_string();
    let responses = workspace.handle_lsp_message(&request);
    let response: Value = serde_json::from_str(&responses[0]).unwrap();
    let hints = response["result"].as_array().unwrap();

    assert_eq!(hints.len(), 2);
    assert_eq!(hints[0]["label"], ": i32");
    assert_eq!(hints[0]["position"]["line"], 1);
    assert_eq!(hints[0]["position"]["character"], 13);
    assert_eq!(hints[1]["label"], ": bool");
    assert_eq!(hints[1]["position"]["line"], 2);
    assert_eq!(hints[1]["position"]["character"], 13);
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
    assert!(
        !response_val["result"].is_null(),
        "Expected Hover response, got null"
    );
}

#[test]
fn test_lsp_semantic_tokens() {
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

    let tokens_req = json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "textDocument/semanticTokens/full",
        "params": {
            "textDocument": {
                "uri": "file:///src/main.gfs"
            }
        }
    })
    .to_string();

    let responses = workspace.handle_lsp_message(&tokens_req);
    assert_eq!(responses.len(), 1);
    let response_val: Value = serde_json::from_str(&responses[0]).unwrap();
    assert_eq!(response_val["id"], 3);
    assert!(!response_val["result"].is_null());
    assert!(
        !response_val["result"]["data"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn test_lsp_goto_definition() {
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

    let open_req = json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": "file:///src/main.gfs",
                "languageId": "galfus",
                "version": 1,
                "text": "fn my_func(): null {} \n fn main(): null { my_func() }"
            }
        }
    })
    .to_string();
    let _ = workspace.handle_lsp_message(&open_req);

    let def_req = json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "textDocument/definition",
        "params": {
            "textDocument": {
                "uri": "file:///src/main.gfs"
            },
            "position": {
                "line": 1,
                "character": 22 // hovering over 'my_func' call
            }
        }
    })
    .to_string();

    let responses = workspace.handle_lsp_message(&def_req);
    assert_eq!(responses.len(), 1);
    let response_val: Value = serde_json::from_str(&responses[0]).unwrap();
    assert_eq!(response_val["id"], 4);
    assert!(!response_val["result"].is_null());

    // Check if location points to the declaration line 0
    let loc = &response_val["result"];
    assert_eq!(loc["range"]["start"]["line"], 0);
}
