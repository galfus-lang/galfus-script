#[cfg(test)]
mod tests;

pub mod definition;
pub mod diagnostics;
pub mod hover;
pub mod rpc;
pub mod semantic_tokens;

use crate::workspace::Workspace;
use galfus_core::{ModulePath, SourceFile};
use lsp_types::{
    DidChangeTextDocumentParams, DidOpenTextDocumentParams, GotoDefinitionParams, HoverParams,
    PublishDiagnosticsParams, SemanticTokensParams,
};
use rpc::{JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};
use serde_json::Value;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn uri_to_file_path(uri: &lsp_types::Url) -> Result<std::path::PathBuf, ()> {
    uri.to_file_path()
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn uri_to_file_path(uri: &lsp_types::Url) -> Result<std::path::PathBuf, ()> {
    Ok(std::path::PathBuf::from(uri.path()))
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn file_path_to_uri(path: &std::path::Path) -> Result<lsp_types::Url, ()> {
    lsp_types::Url::from_file_path(path)
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn file_path_to_uri(path: &std::path::Path) -> Result<lsp_types::Url, ()> {
    let s = path.to_string_lossy();
    let s = if s.starts_with('/') {
        s.to_string()
    } else {
        format!("/{}", s)
    };
    lsp_types::Url::parse(&format!("file://{}", s)).map_err(|_| ())
}

impl Workspace {
    fn uri_to_module_path(&self, uri: &lsp_types::Url) -> Option<String> {
        if uri.scheme() == "galfus" && uri.host_str() == Some("virtual") {
            return Some(uri.path().trim_start_matches('/').to_string());
        }

        if let Ok(file_path) = uri_to_file_path(uri) {
            if let Some(root) = &self.root_path {
                let root_path = root.canonicalize().unwrap_or_else(|_| root.clone());
                let canonical_file = file_path
                    .canonicalize()
                    .unwrap_or_else(|_| file_path.clone());
                if let Ok(stripped) = canonical_file.strip_prefix(&root_path) {
                    return Some(stripped.to_string_lossy().replace('\\', "/"));
                }
            }
            // fallback
            let s = file_path.to_string_lossy().replace('\\', "/");
            let s = s.trim_start_matches('/').to_string();
            return Some(s);
        }
        None
    }

    pub fn handle_lsp_message(&mut self, json: &str) -> Vec<String> {
        let mut responses = Vec::new();
        let request: JsonRpcRequest = match serde_json::from_str(json) {
            Ok(req) => req,
            Err(_) => {
                let err_resp = JsonRpcResponse::error(Value::Null, -32700, "Parse error".into());
                if let Ok(s) = serde_json::to_string(&err_resp) {
                    responses.push(s);
                }
                return responses;
            }
        };

        if let Some(id) = request.id {
            if let Some(response) = self.dispatch_lsp_request(&request.method, request.params, id) {
                if let Ok(s) = serde_json::to_string(&response) {
                    responses.push(s);
                }
            }
        } else {
            self.dispatch_lsp_notification(&request.method, request.params, &mut responses);
        }

        responses
    }

    fn dispatch_lsp_request(
        &mut self,
        method: &str,
        params: Option<Value>,
        id: Value,
    ) -> Option<JsonRpcResponse> {
        match method {
            "initialize" => {
                if let Some(p) = params.as_ref() {
                    if let Ok(init_params) =
                        serde_json::from_value::<lsp_types::InitializeParams>(p.clone())
                    {
                        let root_uri = init_params
                            .workspace_folders
                            .and_then(|folders| folders.first().map(|f| f.uri.clone()));

                        if let Some(uri) = root_uri {
                            if let Ok(file_path) = uri_to_file_path(&uri) {
                                self.root_path = Some(file_path.clone());
                                let manifest_path = file_path.join("galfus.toml");
                                if let Ok(manifest_str) = std::fs::read_to_string(manifest_path) {
                                    if let Ok(manifest) = toml::from_str(&manifest_str) {
                                        let _ = self.load_manifest(manifest);
                                    }
                                }
                            }
                        }
                    }
                }

                let result = serde_json::json!({
                    "capabilities": {
                        "hoverProvider": true,
                        "textDocumentSync": 1, // Full sync
                        "definitionProvider": true,
                        "semanticTokensProvider": {
                            "legend": {
                                "tokenTypes": [
                                    "namespace", "type", "class", "enum", "interface",
                                    "struct", "typeParameter", "parameter", "variable",
                                    "property", "enumMember", "event", "function",
                                    "method", "macro", "keyword", "modifier", "comment",
                                    "string", "number", "regexp", "operator"
                                ],
                                "tokenModifiers": [
                                    "declaration", "definition", "readonly", "static",
                                    "deprecated", "abstract", "async", "modification",
                                    "documentation", "defaultLibrary"
                                ]
                            },
                            "full": true
                        }
                    }
                });
                Some(JsonRpcResponse::success(id, result))
            }
            "shutdown" => Some(JsonRpcResponse::success(id, Value::Null)),
            "textDocument/hover" => {
                if let Some(p) = params {
                    if let Ok(hover_params) = serde_json::from_value::<HoverParams>(p) {
                        let uri = &hover_params.text_document_position_params.text_document.uri;
                        if let Some(path_str) = self.uri_to_module_path(uri) {
                            if let Some(hover) = hover::hover(
                                self,
                                &path_str,
                                hover_params.text_document_position_params.position,
                            ) {
                                return Some(JsonRpcResponse::success(
                                    id,
                                    serde_json::to_value(hover).unwrap(),
                                ));
                            }
                        }
                    }
                }
                Some(JsonRpcResponse::success(id, Value::Null))
            }
            "textDocument/definition" => {
                if let Some(p) = params {
                    if let Ok(def_params) = serde_json::from_value::<GotoDefinitionParams>(p) {
                        let uri = &def_params.text_document_position_params.text_document.uri;
                        if let Some(path_str) = self.uri_to_module_path(uri) {
                            if let Some(loc) = definition::goto_definition(
                                self,
                                &path_str,
                                def_params.text_document_position_params.position,
                            ) {
                                return Some(JsonRpcResponse::success(
                                    id,
                                    serde_json::to_value(loc).unwrap(),
                                ));
                            }
                        }
                    }
                }
                Some(JsonRpcResponse::success(id, Value::Null))
            }
            "textDocument/semanticTokens/full" => {
                if let Some(p) = params {
                    if let Ok(st_params) = serde_json::from_value::<SemanticTokensParams>(p) {
                        let uri = &st_params.text_document.uri;
                        if let Some(path_str) = self.uri_to_module_path(uri) {
                            if let Some(tokens) =
                                semantic_tokens::semantic_tokens_full(self, &path_str)
                            {
                                return Some(JsonRpcResponse::success(
                                    id,
                                    serde_json::to_value(tokens).unwrap(),
                                ));
                            }
                        }
                    }
                }
                Some(JsonRpcResponse::success(id, Value::Null))
            }
            "galfus/virtualDocument" => {
                if let Some(p) = params {
                    if let Some(uri_str) = p.get("uri").and_then(|v| v.as_str()) {
                        if let Ok(uri) = lsp_types::Url::parse(uri_str) {
                            let path = uri.path().trim_start_matches("/virtual/");
                            if let Some(module_path) = galfus_core::ModulePath::new(path) {
                                if let Some(entry) = self.source_state.store.get(&module_path) {
                                    let text = String::from_utf8_lossy(&entry.bytes).to_string();
                                    return Some(JsonRpcResponse::success(
                                        id,
                                        serde_json::json!({ "text": text }),
                                    ));
                                }
                            }
                        }
                    }
                }
                Some(JsonRpcResponse::error(
                    id,
                    -32602,
                    "Invalid URI or document not found".into(),
                ))
            }
            _ => Some(JsonRpcResponse::error(
                id,
                -32601,
                format!("Method not found: {}", method),
            )),
        }
    }

    fn dispatch_lsp_notification(
        &mut self,
        method: &str,
        params: Option<Value>,
        responses: &mut Vec<String>,
    ) {
        match method {
            "textDocument/didOpen" => {
                if let Some(p) = params {
                    if let Ok(open_params) = serde_json::from_value::<DidOpenTextDocumentParams>(p)
                    {
                        let uri = &open_params.text_document.uri;
                        if let Some(path_str) = self.uri_to_module_path(uri) {
                            if let Some(_) = ModulePath::new(&path_str) {
                                let _ = self.load_module(
                                    &path_str,
                                    open_params.text_document.text.as_bytes(),
                                );
                                self.check_and_publish_diagnostics(uri, &path_str, responses);
                            }
                        }
                    }
                }
            }
            "textDocument/didChange" => {
                if let Some(p) = params {
                    if let Ok(change_params) =
                        serde_json::from_value::<DidChangeTextDocumentParams>(p)
                    {
                        let uri = &change_params.text_document.uri;
                        if let Some(path_str) = self.uri_to_module_path(uri) {
                            if let Some(_) = ModulePath::new(&path_str) {
                                if let Some(change) = change_params.content_changes.first() {
                                    let _ = self.load_module(&path_str, change.text.as_bytes());
                                    self.check_and_publish_diagnostics(uri, &path_str, responses);
                                }
                            }
                        }
                    }
                }
            }
            "initialized" | "exit" => {}
            _ => {} // Ignore unhandled notifications
        }
    }

    fn check_and_publish_diagnostics(
        &mut self,
        uri: &lsp_types::Url,
        path_str: &str,
        responses: &mut Vec<String>,
    ) {
        let report = self.check();
        let diagnostics = report.diagnostics.clone();

        if let Some(module_path) = ModulePath::new(path_str) {
            let mut lsp_diagnostics = Vec::new();

            if let Some(entry) = self.source_state.store.get(&module_path) {
                let text = String::from_utf8_lossy(&entry.bytes).to_string();
                let source =
                    SourceFile::new(entry.source_id, entry.path.as_str().to_string(), text);

                for diagnostic in diagnostics.iter() {
                    // Only publish diagnostics that belong to this file.
                    if diagnostic.span().source_id() == entry.source_id {
                        lsp_diagnostics.push(diagnostics::convert_diagnostic(diagnostic, &source));
                    }
                }
            }

            let params = PublishDiagnosticsParams {
                uri: uri.clone(),
                diagnostics: lsp_diagnostics,
                version: None,
            };

            let notification = JsonRpcNotification {
                jsonrpc: "2.0".to_string(),
                method: "textDocument/publishDiagnostics".to_string(),
                params: Some(serde_json::to_value(params).unwrap()),
            };

            if let Ok(s) = serde_json::to_string(&notification) {
                responses.push(s);
            }
        }
    }
}
