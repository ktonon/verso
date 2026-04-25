import * as fs from "fs";
import * as path from "path";
import {
  CancellationToken,
  DocumentLink,
  DocumentLinkProvider,
  ExtensionContext,
  languages,
  Position,
  Range,
  TextDocument,
  Uri,
  workspace,
} from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from "vscode-languageclient/node";

let client: LanguageClient;

function findServerPath(): string {
  // Check user setting first
  const config = workspace.getConfiguration("ogma");
  const configPath = config.get<string>("serverPath");
  if (configPath) {
    return configPath;
  }

  // Check workspace target/release/
  const workspaceFolders = workspace.workspaceFolders;
  if (workspaceFolders && workspaceFolders.length > 0) {
    const wsRoot = workspaceFolders[0].uri.fsPath;

    // Direct: workspace is the ogma repo
    const direct = path.join(wsRoot, "target", "release", "ogma");
    if (fs.existsSync(direct)) {
      return direct;
    }

    // Sibling: workspace is next to the ogma repo (e.g. ../erd/target/release/)
    const parent = path.dirname(wsRoot);
    try {
      for (const sibling of fs.readdirSync(parent)) {
        const candidate = path.join(parent, sibling, "target", "release", "ogma");
        if (fs.existsSync(candidate)) {
          return candidate;
        }
      }
    } catch {
      // parent not readable, skip
    }
  }

  // Default: assume it's in PATH
  return "ogma";
}

const includeLinkProvider: DocumentLinkProvider = {
  provideDocumentLinks(
    document: TextDocument,
    _token: CancellationToken
  ): DocumentLink[] {
    const links: DocumentLink[] = [];
    const docDir = path.dirname(document.uri.fsPath);
    const re = /^\s*(?:!include|!bibliography|use)\s+(\S.*?)\s*$/;
    for (let i = 0; i < document.lineCount; i++) {
      const line = document.lineAt(i).text;
      const m = re.exec(line);
      if (!m) continue;
      const target = m[1];
      const start = line.indexOf(target, m[0].indexOf(target));
      if (start < 0) continue;
      const range = new Range(
        new Position(i, start),
        new Position(i, start + target.length)
      );
      const resolved = path.isAbsolute(target)
        ? target
        : path.join(docDir, target);
      links.push(new DocumentLink(range, Uri.file(resolved)));
    }
    return links;
  },
};

export function activate(context: ExtensionContext) {
  context.subscriptions.push(
    languages.registerDocumentLinkProvider(
      { scheme: "file", language: "ogma" },
      includeLinkProvider
    )
  );

  const serverPath = findServerPath();

  const serverOptions: ServerOptions = {
    command: serverPath,
    args: ["lsp"],
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "ogma" }],
  };

  client = new LanguageClient(
    "ogma-lsp",
    "Ogma Language Server",
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
