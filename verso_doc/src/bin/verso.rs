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
    /// Build a .verso document to PDF or LaTeX
    Build {
        /// .verso file to build
        file: String,
        /// Output file. Use .pdf for PDF (default), .tex for LaTeX only
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Watch .verso files and re-verify on save
    Watch {
        /// .verso files to watch
        #[arg(required = true)]
        files: Vec<String>,
    },
    /// Remove cached build artifacts
    Clean,
    /// Start the language server (LSP)
    Lsp,
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Check { files } => cmd_check(&files),
        Command::Build { file, output } => cmd_build(&file, output.as_deref()),
        Command::Watch { files } => cmd_watch(&files),
        Command::Clean => cmd_clean(),
        Command::Lsp => cmd_lsp(),
    }
}

fn cmd_clean() {
    let tmp = std::env::temp_dir().join("verso-build");
    if tmp.exists() {
        if let Err(e) = std::fs::remove_dir_all(&tmp) {
            eprintln!("error removing {}: {}", tmp.display(), e);
            process::exit(1);
        }
        eprintln!("removed {}", tmp.display());
    } else {
        eprintln!("nothing to clean");
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

fn cmd_build(file: &str, output: Option<&str>) {
    use verso_doc::compile_tex::compile_to_tex;
    use verso_doc::parse::parse_document_from_file;
    use std::process::Command;

    let path = Path::new(file);

    // Determine output path and format
    let output_path = match output {
        Some(o) => std::path::PathBuf::from(o),
        None => {
            let stem = path.file_stem().unwrap_or_default().to_string_lossy();
            path.parent().unwrap_or(Path::new(".")).join(format!("{}.pdf", stem))
        }
    };
    let is_tex = output_path.extension().map_or(false, |e| e == "tex");

    let doc = match parse_document_from_file(path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: {}: {}", file, e);
            process::exit(1);
        }
    };

    let tex = compile_to_tex(&doc);

    // LaTeX output — just write the .tex file
    if is_tex {
        if let Err(e) = std::fs::write(&output_path, &tex) {
            eprintln!("error writing {}: {}", output_path.display(), e);
            process::exit(1);
        }
        eprintln!("wrote {}", output_path.display());
        return;
    }

    // PDF output — check for required tools
    let missing: Vec<&str> = ["pdflatex", "bibtex"]
        .iter()
        .copied()
        .filter(|cmd| {
            Command::new("which")
                .arg(cmd)
                .output()
                .map(|o| !o.status.success())
                .unwrap_or(true)
        })
        .collect();

    if !missing.is_empty() {
        eprintln!("error: missing required tools: {}", missing.join(", "));
        eprintln!();
        eprintln!("Install a TeX distribution to get pdflatex and bibtex:");
        eprintln!("  macOS:         brew install --cask basictex");
        eprintln!("  Ubuntu/Debian: sudo apt install texlive-latex-base");
        eprintln!("  Fedora:        sudo dnf install texlive-scheme-basic");
        eprintln!("  Arch:          sudo pacman -S texlive-basic");
        process::exit(1);
    }

    // Build in a temp directory to keep source tree clean
    let tmp = std::env::temp_dir().join("verso-build");
    std::fs::create_dir_all(&tmp).unwrap_or_else(|e| {
        eprintln!("error creating temp dir: {}", e);
        process::exit(1);
    });

    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let tex_file = format!("{}.tex", stem);
    let tex_path = tmp.join(&tex_file);
    std::fs::write(&tex_path, &tex).unwrap_or_else(|e| {
        eprintln!("error writing tex: {}", e);
        process::exit(1);
    });

    // Copy .bib files from source directory to temp build directory
    if let Some(src_dir) = path.parent() {
        let abs_src = std::fs::canonicalize(src_dir).unwrap_or_else(|_| src_dir.to_path_buf());
        if let Ok(entries) = std::fs::read_dir(&abs_src) {
            for entry in entries.flatten() {
                if entry.path().extension().map_or(false, |e| e == "bib") {
                    let dest = tmp.join(entry.file_name());
                    let _ = std::fs::copy(entry.path(), dest);
                }
            }
        }
    }

    let run = |cmd: &str, args: &[&str]| -> bool {
        let status = Command::new(cmd)
            .args(args)
            .current_dir(&tmp)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        match status {
            Ok(s) => s.success(),
            Err(e) => {
                eprintln!("error running {}: {}", cmd, e);
                false
            }
        }
    };

    // pdflatex → bibtex → pdflatex → pdflatex
    if !run("pdflatex", &["-interaction=nonstopmode", &tex_file]) {
        eprintln!("error: pdflatex failed (pass 1)");
        process::exit(1);
    }
    // bibtex may fail if there are no citations — that's ok
    let _ = run("bibtex", &[stem.as_ref()]);
    if !run("pdflatex", &["-interaction=nonstopmode", &tex_file]) {
        eprintln!("error: pdflatex failed (pass 2)");
        process::exit(1);
    }
    if !run("pdflatex", &["-interaction=nonstopmode", &tex_file]) {
        eprintln!("error: pdflatex failed (pass 3)");
        process::exit(1);
    }

    let built_pdf = tmp.join(format!("{}.pdf", stem));
    let abs_output = std::fs::canonicalize(output_path.parent().unwrap_or(Path::new(".")))
        .unwrap_or_else(|_| output_path.parent().unwrap_or(Path::new(".")).to_path_buf())
        .join(output_path.file_name().unwrap_or_default());
    std::fs::copy(&built_pdf, &abs_output).unwrap_or_else(|e| {
        eprintln!("error copying PDF: {}", e);
        process::exit(1);
    });

    eprintln!("wrote {}", abs_output.display());
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
