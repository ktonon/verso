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
  const config = workspace.getConfiguration("verso");
  const configPath = config.get<string>("serverPath");
  if (configPath) {
    return configPath;
  }

  // Check workspace target/release/
  const workspaceFolders = workspace.workspaceFolders;
  if (workspaceFolders && workspaceFolders.length > 0) {
    const wsRoot = workspaceFolders[0].uri.fsPath;

    // Direct: workspace is the verso repo
    const direct = path.join(wsRoot, "target", "release", "verso_lsp");
    if (fs.existsSync(direct)) {
      return direct;
    }

    // Sibling: workspace is next to the verso repo (e.g. ../erd/target/release/)
    const parent = path.dirname(wsRoot);
    try {
      for (const sibling of fs.readdirSync(parent)) {
        const candidate = path.join(parent, sibling, "target", "release", "verso_lsp");
        if (fs.existsSync(candidate)) {
          return candidate;
        }
      }
    } catch {
      // parent not readable, skip
    }
  }

  // Default: assume it's in PATH
  return "verso_lsp";
}

export function activate(context: ExtensionContext) {
  const serverPath = findServerPath();

  const serverOptions: ServerOptions = {
    command: serverPath,
    args: [],
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "verso" }],
  };

  client = new LanguageClient(
    "verso-lsp",
    "Verso Language Server",
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
