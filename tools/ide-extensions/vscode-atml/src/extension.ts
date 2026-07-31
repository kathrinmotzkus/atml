import * as fs from 'node:fs';
import * as path from 'node:path';
import * as vscode from 'vscode';
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  context.subscriptions.push(
    vscode.commands.registerCommand('atml.restartServer', restartServer),
  );
  await startServer(context);
}

export async function deactivate(): Promise<void> {
  if (client) {
    await client.stop();
    client = undefined;
  }
}

async function restartServer(): Promise<void> {
  if (!client) {
    return;
  }
  await client.restart();
}

async function startServer(context: vscode.ExtensionContext): Promise<void> {
  const serverOptions = resolveServerOptions(context);
  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: 'file', language: 'atml' }],
    synchronize: {
      configurationSection: 'atml',
      fileEvents: vscode.workspace.createFileSystemWatcher('**/*.atml'),
    },
    outputChannelName: 'ATML Language Server',
  };

  client = new LanguageClient(
    'atmlLanguageServer',
    'ATML Language Server',
    serverOptions,
    clientOptions,
  );
  await client.start();
}

function resolveServerOptions(context: vscode.ExtensionContext): ServerOptions {
  const configured = vscode.workspace
    .getConfiguration('atml')
    .get<string>('server.path', '')
    .trim();
  if (configured) {
    return { command: configured };
  }

  const executable = process.platform === 'win32'
    ? 'atml-language-server.exe'
    : 'atml-language-server';
  const bundled = context.asAbsolutePath(path.join('bin', executable));
  if (fs.existsSync(bundled)) {
    return { command: bundled };
  }

  // Development checkout: run the workspace binary directly through Cargo.
  const manifest = context.asAbsolutePath(path.join('..', 'Cargo.toml'));
  if (fs.existsSync(manifest)) {
    return {
      command: 'cargo',
      args: [
        'run',
        '--quiet',
        '--manifest-path',
        manifest,
        '-p',
        'atml-language-server',
        '--',
      ],
    };
  }

  throw new Error(
    'ATML language server not found. Configure atml.server.path or reinstall the extension.',
  );
}
