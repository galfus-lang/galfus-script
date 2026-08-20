import { EditorState } from "https://esm.sh/@codemirror/state";
import { EditorView } from "https://esm.sh/@codemirror/view";
import { linter, lintGutter } from "https://esm.sh/@codemirror/lint";
import { basicSetup } from "https://esm.sh/codemirror";

import init, { Playground } from "./wasm/galfus_playground_web.js";

async function main() {
  await init();
  const playground = new Playground();

  // Initialize the LSP server
  playground.handleLspMessage(
    JSON.stringify({
      jsonrpc: "2.0",
      id: 1,
      method: "initialize",
      params: {
        processId: null,
        rootUri: null,
        capabilities: {},
      },
    }),
  );

  let documentVersion = 1;
  const initialDoc = "fn main() {\n  let a\n}";

  // Simulate opening the file
  playground.handleLspMessage(
    JSON.stringify({
      jsonrpc: "2.0",
      method: "textDocument/didOpen",
      params: {
        textDocument: {
          uri: "galfus://virtual/src/main.gfs",
          languageId: "galfus",
          version: documentVersion,
          text: initialDoc,
        },
      },
    }),
  );

  // Create the Linter extension bridging CodeMirror and our WASM LSP
  const galfusLinter = linter(async (view) => {
    const text = view.state.doc.toString();
    documentVersion++;

    const responses = playground.handleLspMessage(
      JSON.stringify({
        jsonrpc: "2.0",
        method: "textDocument/didChange",
        params: {
          textDocument: {
            uri: "galfus://virtual/src/main.gfs",
            version: documentVersion,
          },
          contentChanges: [{ text }],
        },
      }),
    );

    let lspDiagnostics = [];
    for (const json of responses) {
      const msg = JSON.parse(json);
      if (msg.method === "textDocument/publishDiagnostics") {
        lspDiagnostics = msg.params.diagnostics;
      }
    }

    return lspDiagnostics.map((d) => {
      let from = 0;
      let to = 0;
      try {
        from =
          view.state.doc.line(d.range.start.line + 1).from +
          d.range.start.character;
        to =
          view.state.doc.line(d.range.end.line + 1).from +
          d.range.end.character;
      } catch (e) {
        console.error("Offset mapping failed:", e);
      }
      return {
        from,
        to,
        severity:
          d.severity === 2
            ? "warning"
            : d.severity === 3
              ? "info"
              : d.severity === 4
                ? "hint"
                : "error",
        message: d.message,
        source: d.source || "galfus-lsp",
      };
    });
  });

  const state = EditorState.create({
    doc: initialDoc,
    extensions: [
      basicSetup,
      lintGutter(),
      galfusLinter,
      EditorView.theme(
        {
          "&": { height: "100%" },
          ".cm-scroller": { overflow: "auto" },
        },
        { dark: true },
      ),
    ],
  });

  new EditorView({
    state,
    parent: document.getElementById("editor"),
  });
}

main().catch(console.error);
