import * as vscode from 'vscode';
import * as cp from 'child_process';
import { LanguageClient } from 'vscode-languageclient/node';
import type {
  LanguageClientOptions,
  ServerOptions,
} from 'vscode-languageclient/node';

let client: LanguageClient;

export function activate(context: vscode.ExtensionContext) {
  let versionOutput = "";
  // Check if galfus is installed
  try {
    versionOutput = cp.execSync('galfus --version', { encoding: 'utf-8' });
  } catch (err) {
    vscode.window.showErrorMessage(
      "Galfus CLI not found in PATH. Please install Galfus Script to use language features."
    );
    return;
  }

  // Fire and forget update check
  checkForUpdates(versionOutput);

  const parts = versionOutput.trim().split(' ');
  const fullVersion = parts[1]; // e.g. "0.2.4-alpha" or "0.2.4"
  if (fullVersion) {
    const versionOnly = fullVersion.includes('-') ? fullVersion.split('-')[0] : fullVersion;
    if (versionOnly && !isVersionGreaterOrEqual(versionOnly, '0.3.0')) {
      vscode.window.showWarningMessage(
        `Galfus Language Server requires CLI version 0.3.0 or higher. You are currently using ${fullVersion}. Please upgrade to use language features.`
      );
      return;
    }
  }

  const serverOptions: ServerOptions = {
    command: 'galfus',
    args: ['lsp'],
    options: {
      env: process.env,
    },
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: 'file', language: 'galfus' }],
    synchronize: {
      fileEvents: vscode.workspace.createFileSystemWatcher('**/*.gfs'),
    },
  };

  client = new LanguageClient(
    'galfusLanguageServer',
    'Galfus Language Server',
    serverOptions,
    clientOptions
  );

  client.start();
}

export function deactivate(): Thenable<void> | undefined {
  if (!client) {
    return undefined;
  }
  return client.stop();
}

function checkForUpdates(versionOutput: string) {
  const parts = versionOutput.trim().split(' ');
  const fullVersion = parts[1]; // e.g. "0.2.4-alpha" or "0.2.4"
  if (!fullVersion) { return; }

  let version = fullVersion;
  let tag = 'latest';

  if (fullVersion.includes('-')) {
    const vParts = fullVersion.split('-');
    version = vParts[0] ?? fullVersion;
    tag = vParts.slice(1).join('-');
  }

  fetch('https://storage.galfus.com/manifest.json')
    .then((res) => res.json())
    .then((manifest: any) => {
      const resolvedTag = tag === 'latest' ? manifest.latest_tag : tag;
      const remoteVersion = manifest.tags[resolvedTag];

      if (remoteVersion && remoteVersion !== version) {
        vscode.window
          .showInformationMessage(
            `A new Galfus update is available (${remoteVersion}-${resolvedTag}).`,
            'Upgrade Now'
          )
          .then((selection) => {
            if (selection === 'Upgrade Now') {
              const terminal = vscode.window.createTerminal('Galfus Upgrade');
              terminal.show();
              terminal.sendText(`galfus upgrade --tag ${resolvedTag}`);
            }
          });
      }
    })
    .catch((err) => console.error('Failed to check for Galfus updates:', err));
}

function isVersionGreaterOrEqual(current: string, minimum: string): boolean {
  const parse = (v: string) => v.split('.').map(Number);
  const curParts = parse(current);
  const minParts = parse(minimum);

  const len = Math.max(curParts.length, minParts.length);
  for (let i = 0; i < len; i++) {
    const c = curParts[i] || 0;
    const m = minParts[i] || 0;
    if (c > m) { return true; }
    if (c < m) { return false; }
  }
  return true;
}
