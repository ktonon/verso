import * as fs from "fs";
import * as path from "path";
import { workspace, ExtensionContext } from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from "vscode-languageclient/node";

let client: LanguageClient;

function findServerPath(): string {
  // Check user setting first
  const config = workspace.getConfiguration("erd");
  const configPath = config.get<string>("serverPath");
  if (configPath) {
    return configPath;
  }

  // Check workspace target/release/ only if it exists
  const workspaceFolders = workspace.workspaceFolders;
  if (workspaceFolders && workspaceFolders.length > 0) {
    const candidate = path.join(
      workspaceFolders[0].uri.fsPath,
      "target",
      "release",
      "erd_lsp"
    );
    if (fs.existsSync(candidate)) {
      return candidate;
    }
  }

  // Default: assume it's in PATH
  return "erd_lsp";
}

export function activate(context: ExtensionContext) {
  const serverPath = findServerPath();

  const serverOptions: ServerOptions = {
    command: serverPath,
    args: [],
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "erd" }],
  };

  client = new LanguageClient(
    "erd-lsp",
    "ERD Language Server",
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
