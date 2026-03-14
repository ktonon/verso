use clap::{Parser, Subcommand};
use std::path::Path;
use std::process;

#[derive(Parser)]
#[command(name = "verso", about = "Verso — verifiable source for scientific papers")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Verify claims in .verso documents
    Check {
        /// .verso files to check
        #[arg(required = true)]
        files: Vec<String>,
    },
    /// Compile .verso documents to LaTeX
    Compile {
        /// .verso file to compile
        file: String,
        /// Output file (default: stdout)
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Watch .verso files and re-verify on save
    Watch {
        /// .verso files to watch
        #[arg(required = true)]
        files: Vec<String>,
    },
    /// Start the language server (LSP)
    Lsp,
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Check { files } => cmd_check(&files),
        Command::Compile { file, output } => cmd_compile(&file, output.as_deref()),
        Command::Watch { files } => cmd_watch(&files),
        Command::Lsp => cmd_lsp(),
    }
}

fn cmd_check(files: &[String]) {
    use verso_doc::parse::parse_document_from_file;
    use verso_doc::report::ReportFormatter;
    use verso_doc::verify::verify_document;

    let mut all_passed = true;

    for file in files {
        let doc = match parse_document_from_file(Path::new(file)) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("error: {}: {}", file, e);
                all_passed = false;
                continue;
            }
        };

        let report = verify_document(&doc);
        let formatter = ReportFormatter {
            report: &report,
            filename: file,
        };
        print!("{}", formatter);

        if !report.all_passed() {
            all_passed = false;
        }
    }

    if !all_passed {
        process::exit(1);
    }
}

fn cmd_compile(file: &str, output: Option<&str>) {
    use verso_doc::compile_tex::compile_to_tex;
    use verso_doc::parse::parse_document_from_file;

    let doc = match parse_document_from_file(Path::new(file)) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: {}: {}", file, e);
            process::exit(1);
        }
    };

    let tex = compile_to_tex(&doc);

    if let Some(output_path) = output {
        if let Err(e) = std::fs::write(output_path, &tex) {
            eprintln!("error writing {}: {}", output_path, e);
            process::exit(1);
        }
        eprintln!("wrote {}", output_path);
    } else {
        print!("{}", tex);
    }
}

fn cmd_watch(files: &[String]) {
    use verso_doc::parse::parse_document_from_file;
    use verso_doc::report::ReportFormatter;
    use verso_doc::verify::verify_document;
    use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
    use std::sync::mpsc;
    use std::time::Duration;

    let check = |files: &[String]| {
        print!("\x1b[2J\x1b[H");
        let mut all_passed = true;
        for file in files {
            let doc = match parse_document_from_file(Path::new(file)) {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("error: {}: {}", file, e);
                    all_passed = false;
                    continue;
                }
            };
            let report = verify_document(&doc);
            let formatter = ReportFormatter {
                report: &report,
                filename: file,
            };
            print!("{}", formatter);
            if !report.all_passed() {
                all_passed = false;
            }
        }
        if all_passed {
            println!("\n\x1b[32mWatching for changes... (Ctrl+C to stop)\x1b[0m");
        } else {
            println!("\n\x1b[31mWatching for changes... (Ctrl+C to stop)\x1b[0m");
        }
    };

    check(files);

    let (tx, rx) = mpsc::channel();
    let mut debouncer = new_debouncer(Duration::from_millis(300), tx)
        .expect("failed to create file watcher");

    for path in files {
        debouncer
            .watcher()
            .watch(Path::new(path), notify::RecursiveMode::NonRecursive)
            .unwrap_or_else(|e| {
                eprintln!("warning: cannot watch {}: {}", path, e);
            });
    }

    loop {
        match rx.recv() {
            Ok(Ok(events)) => {
                let has_write = events
                    .iter()
                    .any(|e| matches!(e.kind, DebouncedEventKind::Any));
                if has_write {
                    check(files);
                }
            }
            Ok(Err(e)) => eprintln!("watch error: {}", e),
            Err(_) => break,
        }
    }
}

#[tokio::main]
async fn cmd_lsp() {
    use verso_doc::compile_tex::find_unresolved_refs_against;
    use verso_doc::dim::DimOutcome;
    use verso_doc::parse::{parse_document, parse_document_from_file};
    use verso_doc::verify::{verify_document, Outcome};
    use std::path::PathBuf;
    use tower_lsp::jsonrpc::Result;
    use tower_lsp::lsp_types::*;
    use tower_lsp::{Client, LanguageServer, LspService, Server};

    struct VersoServer {
        client: Client,
    }

    #[tower_lsp::async_trait]
    impl LanguageServer for VersoServer {
        async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
            Ok(InitializeResult {
                capabilities: ServerCapabilities {
                    text_document_sync: Some(TextDocumentSyncCapability::Options(
                        TextDocumentSyncOptions {
                            open_close: Some(true),
                            change: Some(TextDocumentSyncKind::FULL),
                            save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                                include_text: Some(true),
                            })),
                            ..Default::default()
                        },
                    )),
                    ..Default::default()
                },
                ..Default::default()
            })
        }

        async fn initialized(&self, _: InitializedParams) {
            self.client
                .log_message(MessageType::INFO, "verso-lsp initialized")
                .await;
        }

        async fn shutdown(&self) -> Result<()> {
            Ok(())
        }

        async fn did_open(&self, params: DidOpenTextDocumentParams) {
            let file_path = params.text_document.uri.to_file_path().ok();
            let diagnostics = compute_diagnostics(&params.text_document.text, file_path.as_deref());
            self.client
                .publish_diagnostics(params.text_document.uri, diagnostics, None)
                .await;
        }

        async fn did_change(&self, params: DidChangeTextDocumentParams) {
            if let Some(change) = params.content_changes.into_iter().last() {
                let diagnostics = compute_diagnostics(&change.text, None);
                self.client
                    .publish_diagnostics(params.text_document.uri, diagnostics, None)
                    .await;
            }
        }

        async fn did_save(&self, params: DidSaveTextDocumentParams) {
            if let Some(text) = params.text {
                let file_path = params.text_document.uri.to_file_path().ok();
                let diagnostics = compute_diagnostics(&text, file_path.as_deref());
                self.client
                    .publish_diagnostics(params.text_document.uri, diagnostics, None)
                    .await;
            }
        }
    }

    fn compute_diagnostics(text: &str, file_path: Option<&Path>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        let doc = match parse_document(text) {
            Ok(d) => d,
            Err(e) => {
                diagnostics.push(Diagnostic {
                    range: line_range(e.line),
                    severity: Some(DiagnosticSeverity::ERROR),
                    message: e.message,
                    source: Some("verso".to_string()),
                    ..Default::default()
                });
                return diagnostics;
            }
        };

        let report = verify_document(&doc);

        for result in &report.results {
            match &result.outcome {
                Outcome::Pass | Outcome::ProofPass { .. } => {}
                Outcome::NumericalPass {
                    samples, residual, ..
                } => {
                    diagnostics.push(Diagnostic {
                        range: line_range(result.span.line),
                        severity: Some(DiagnosticSeverity::WARNING),
                        message: format!(
                            "'{}' passed numerically ({} samples) but not symbolically. Residual: {}",
                            result.name, samples, residual
                        ),
                        source: Some("verso".to_string()),
                        ..Default::default()
                    });
                }
                Outcome::Fail { residual } => {
                    diagnostics.push(Diagnostic {
                        range: line_range(result.span.line),
                        severity: Some(DiagnosticSeverity::ERROR),
                        message: format!("'{}' failed. Residual: {}", result.name, residual),
                        source: Some("verso".to_string()),
                        ..Default::default()
                    });
                }
                Outcome::ProofStepFail {
                    step_index,
                    from,
                    to,
                    residual,
                    step_span,
                } => {
                    diagnostics.push(Diagnostic {
                        range: line_range(step_span.line),
                        severity: Some(DiagnosticSeverity::ERROR),
                        message: format!(
                            "'{}' step {} failed: {} \u{2260} {}. Residual: {}",
                            result.name, step_index, from, to, residual
                        ),
                        source: Some("verso".to_string()),
                        ..Default::default()
                    });
                }
            }

            if let Some(ref dim) = result.dim_outcome {
                match dim {
                    DimOutcome::Pass | DimOutcome::Skipped { .. } => {}
                    DimOutcome::LhsRhsMismatch { lhs, rhs } => {
                        diagnostics.push(Diagnostic {
                            range: line_range(result.span.line),
                            severity: Some(DiagnosticSeverity::ERROR),
                            message: format!(
                                "'{}' dimension mismatch: lhs {}, rhs {}",
                                result.name, lhs, rhs
                            ),
                            source: Some("verso".to_string()),
                            ..Default::default()
                        });
                    }
                    DimOutcome::ExprError { side, error } => {
                        diagnostics.push(Diagnostic {
                            range: line_range(result.span.line),
                            severity: Some(DiagnosticSeverity::ERROR),
                            message: format!(
                                "'{}' dimension error in {}: {}",
                                result.name, side, error
                            ),
                            source: Some("verso".to_string()),
                            ..Default::default()
                        });
                    }
                }
            }
        }

        if let Some(path) = file_path {
            let ref_doc = find_root_document(path)
                .and_then(|root| parse_document_from_file(&root).ok());
            let check_doc = ref_doc.as_ref().unwrap_or(&doc);
            for label in find_unresolved_refs_against(check_doc, &doc) {
                let line = text.lines().enumerate()
                    .find(|(_, l)| l.contains(&format!("ref`{}", label)))
                    .map(|(i, _)| i + 1)
                    .unwrap_or(0);
                diagnostics.push(Diagnostic {
                    range: line_range(line),
                    severity: Some(DiagnosticSeverity::WARNING),
                    message: format!("unresolved reference: '{}'", label),
                    source: Some("verso".to_string()),
                    ..Default::default()
                });
            }
        }

        diagnostics
    }

    fn line_range(line: usize) -> Range {
        let line = (line.saturating_sub(1)) as u32;
        Range {
            start: Position { line, character: 0 },
            end: Position { line, character: u32::MAX },
        }
    }

    fn find_root_document(file_path: &Path) -> Option<PathBuf> {
        let mut dir = file_path.parent()?;
        loop {
            for name in &["paper.verso", "index.verso"] {
                let candidate = dir.join(name);
                if candidate.exists() && candidate != file_path {
                    return Some(candidate);
                }
            }
            dir = dir.parent()?;
        }
    }

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(|client| VersoServer { client });
    Server::new(stdin, stdout, socket).serve(service).await;
}
