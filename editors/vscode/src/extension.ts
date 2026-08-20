import * as util from 'util';
import * as vscode from 'vscode';
import * as cp from 'child_process';
import { LanguageClient } from 'vscode-languageclient/node';
import type {
  LanguageClientOptions,
  ServerOptions,
} from 'vscode-languageclient/node';

import { readFileSync, existsSync } from 'fs';
import { join } from 'path';

let client: LanguageClient;

export async function activate(context: vscode.ExtensionContext) {
  let versionOutput = "";
  
  let isDevMode = false;
  let cliPath = 'galfus';
  
  const devEnvPath = join(__dirname, '..', '.galfus-dev-env');
  if (existsSync(devEnvPath)) {
    try {
      const env = JSON.parse(readFileSync(devEnvPath, 'utf8'));
      if (env.GALFUS_DEV_MODE) isDevMode = true;
      if (env.GALFUS_CLI_PATH) cliPath = env.GALFUS_CLI_PATH;
    } catch (e) {}
  }

  if (!isDevMode) {
    // Check if galfus is installed
    try {
      const execAsync = util.promisify(cp.exec);
      const { stdout } = await execAsync('galfus --version', { encoding: 'utf-8' });
      versionOutput = stdout;
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
  }

  const serverOptions: ServerOptions = {
    command: cliPath,
    args: ['lsp'],
    options: {
      env: process.env,
    },
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [
      { scheme: 'file', language: 'galfus' },
      { scheme: 'galfus', language: 'galfus' }
    ],
    synchronize: {
      fileEvents: vscode.workspace.createFileSystemWatcher('**/*.gfs'),
    },
    markdown: {
      isTrusted: true
    }
  };

  client = new LanguageClient(
    'galfusLanguageServer',
    'Galfus Language Server',
    serverOptions,
    clientOptions
  );

  client.start();

  const provider = new (class implements vscode.FileSystemProvider {
    onDidChangeFile = new vscode.EventEmitter<vscode.FileChangeEvent[]>().event;
    watch() { return new vscode.Disposable(() => {}); }
    stat(uri: vscode.Uri): vscode.FileStat {
      return {
        type: vscode.FileType.File,
        ctime: Date.now(),
        mtime: Date.now(),
        size: 0,
        permissions: vscode.FilePermission.Readonly,
      };
    }
    readDirectory() { return []; }
    createDirectory() { throw vscode.FileSystemError.NoPermissions(); }
    async readFile(uri: vscode.Uri): Promise<Uint8Array> {
      try {
        const response: any = await client.sendRequest('galfus/virtualDocument', {
          uri: uri.toString(),
        });
        return new TextEncoder().encode(response.text);
      } catch (e) {
        return new TextEncoder().encode(`// Error loading virtual document: ${e}`);
      }
    }
    writeFile() { throw vscode.FileSystemError.NoPermissions(); }
    delete() { throw vscode.FileSystemError.NoPermissions(); }
    rename() { throw vscode.FileSystemError.NoPermissions(); }
  })();

  context.subscriptions.push(
    vscode.workspace.registerFileSystemProvider('galfus', provider, {
      isReadonly: true,
      isCaseSensitive: true,
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('galfus.openVirtual', async (uriStr, line, col) => {
      try {
        const uri = vscode.Uri.parse(uriStr);
        const doc = await vscode.workspace.openTextDocument(uri);
        const editor = await vscode.window.showTextDocument(doc, { preview: true });
        if (line !== undefined && col !== undefined) {
          const pos = new vscode.Position(line - 1, col - 1);
          editor.selection = new vscode.Selection(pos, pos);
          editor.revealRange(new vscode.Range(pos, pos), vscode.TextEditorRevealType.InCenter);
        }
      } catch (e) {
        vscode.window.showErrorMessage(`Failed to open virtual file: ${e}`);
      }
    })
  );
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
