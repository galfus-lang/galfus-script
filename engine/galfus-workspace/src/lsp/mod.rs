#[cfg(test)]
mod tests;

pub mod diagnostics;
pub mod hover;
pub mod rpc;

use crate::workspace::Workspace;
use galfus_core::{ModulePath, SourceFile};
use lsp_types::{
    DidChangeTextDocumentParams, DidOpenTextDocumentParams, HoverParams, PublishDiagnosticsParams,
};
use rpc::{JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};
use serde_json::Value;

impl Workspace {
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
                let result = serde_json::json!({
                    "capabilities": {
                        "hoverProvider": true,
                        "textDocumentSync": 1, // Full sync
                    }
                });
                Some(JsonRpcResponse::success(id, result))
            }
            "shutdown" => Some(JsonRpcResponse::success(id, Value::Null)),
            "textDocument/hover" => {
                if let Some(p) = params {
                    if let Ok(hover_params) = serde_json::from_value::<HoverParams>(p) {
                        let uri = &hover_params.text_document_position_params.text_document.uri;
                        let path_str = uri.path().trim_start_matches('/').to_string();
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
                Some(JsonRpcResponse::success(id, Value::Null))
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
                        let path_str = uri.path().trim_start_matches('/').to_string();

                        if let Some(_) = ModulePath::new(&path_str) {
                            let _ = self
                                .load_module(&path_str, open_params.text_document.text.as_bytes());
                            self.check_and_publish_diagnostics(uri, &path_str, responses);
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
                        let path_str = uri.path().trim_start_matches('/').to_string();

                        if let Some(_) = ModulePath::new(&path_str) {
                            if let Some(change) = change_params.content_changes.first() {
                                let _ = self.load_module(&path_str, change.text.as_bytes());
                                self.check_and_publish_diagnostics(uri, &path_str, responses);
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
