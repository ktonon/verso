use clap::{Parser, Subcommand};
use std::path::Path;
use std::process;

#[derive(Parser)]
#[command(
    name = "verso",
    version,
    about = "Verso — verifiable source for scientific papers"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Verify claims in .verso documents.
    /// With no arguments, reads .verso.jsonc config.
    Check {
        /// .verso files to check (optional if .verso.jsonc exists)
        files: Vec<String>,
        /// Watch files and re-run on save
        #[arg(short, long)]
        watch: bool,
    },
    /// Build .verso documents to PDF or LaTeX.
    /// With no arguments, reads .verso.jsonc config.
    Build {
        /// .verso file to build (optional if .verso.jsonc exists)
        file: Option<String>,
        /// Output file. Use .pdf for PDF (default), .tex for LaTeX only
        #[arg(short, long)]
        output: Option<String>,
        /// Watch files and re-build on save
        #[arg(short, long)]
        watch: bool,
    },
    /// Remove cached build artifacts
    Clean,
    /// Initialize a new verso project (creates .verso.jsonc)
    Init,
    /// Interactive symbolic math REPL
    Repl,
    /// Start the language server (LSP)
    Lsp,
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Check { files, watch } => {
            let files = if files.is_empty() {
                require_config().inputs()
            } else {
                files
            };
            if watch {
                let tasks: Vec<WatchTask> = files
                    .iter()
                    .map(|f| {
                        let f2 = f.clone();
                        WatchTask::new(f, move || cmd_check(&[f2.clone()]))
                    })
                    .collect();
                watch_and_run(tasks);
            } else {
                cmd_check(&files);
                stamp_config_if_present();
            }
        }
        Command::Build {
            file: None,
            output,
            watch,
        } => {
            let config = require_config();
            if watch {
                let tasks: Vec<WatchTask> = config
                    .papers
                    .iter()
                    .map(|paper| {
                        let input = paper.input.clone();
                        let input2 = input.clone();
                        let out = output.clone().unwrap_or_else(|| {
                            format!("{}/{}.pdf", config.output_dir, paper.output)
                        });
                        WatchTask::new(&input, move || cmd_build(&input2, Some(&out)))
                    })
                    .collect();
                // Ensure output dir exists before first run
                if config.output_dir != "." {
                    std::fs::create_dir_all(&config.output_dir).ok();
                }
                watch_and_run(tasks);
            } else {
                cmd_build_from_config_resolved(&config, output.as_deref());
                stamp_config_if_present();
            }
        }
        Command::Build {
            file: Some(f),
            output,
            watch,
        } => {
            if watch {
                let f2 = f.clone();
                let out = output;
                let tasks = vec![WatchTask::new(&f, move || cmd_build(&f2, out.as_deref()))];
                watch_and_run(tasks);
            } else {
                cmd_build(&f, output.as_deref());
            }
        }
        Command::Clean => cmd_clean(),
        Command::Init => cmd_init(),
        Command::Repl => {
            if let Err(e) = verso_symbolic::repl::run() {
                eprintln!("repl error: {}", e);
                process::exit(1);
            }
        }
        Command::Lsp => cmd_lsp(),
    }
}

/// Stamp the config file with the current verso version and schema URL after a successful run.
fn stamp_config_if_present() {
    use verso_doc::config::{find_config, stamp_config};
    if let Some(cwd) = std::env::current_dir().ok() {
        if let Some(path) = find_config(&cwd) {
            if let Err(e) = stamp_config(&path) {
                eprintln!("warning: could not update config: {}", e);
            }
        }
    }
}

fn require_config() -> verso_doc::config::ResolvedConfig {
    use verso_doc::config::resolve_config;

    let cwd = std::env::current_dir().unwrap_or_else(|e| {
        eprintln!("error: cannot determine current directory: {}", e);
        process::exit(1);
    });

    match resolve_config(&cwd) {
        Ok(Some(cfg)) => cfg,
        Ok(None) => {
            eprintln!("error: no .verso.jsonc or .verso.json found");
            eprintln!("run 'verso init' to create one");
            process::exit(1);
        }
        Err(e) => {
            eprintln!("error: {}", e);
            process::exit(1);
        }
    }
}

fn cmd_init() {
    use verso_doc::config::{default_config_content, install_schema, CONFIG_FILENAME};

    let path = Path::new(CONFIG_FILENAME);
    if path.exists() {
        eprintln!("{} already exists", CONFIG_FILENAME);
        process::exit(1);
    }
    let content = default_config_content();
    if let Err(e) = std::fs::write(path, &content) {
        eprintln!("error writing {}: {}", CONFIG_FILENAME, e);
        process::exit(1);
    }
    if let Err(e) = install_schema(path) {
        eprintln!("warning: could not install schema: {}", e);
    }
    eprintln!("created {}", CONFIG_FILENAME);
}

fn cmd_clean() {
    use verso_doc::config::resolve_config;

    let mut cleaned = false;

    let tmp = std::env::temp_dir().join("verso-build");
    if tmp.exists() {
        if let Err(e) = std::fs::remove_dir_all(&tmp) {
            eprintln!("error removing {}: {}", tmp.display(), e);
            process::exit(1);
        }
        eprintln!("removed {}", tmp.display());
        cleaned = true;
    }

    let cwd = std::env::current_dir().ok();
    if let Some(ref cwd) = cwd {
        if let Ok(Some(config)) = resolve_config(cwd) {
            if config.output_dir != "." {
                let out = Path::new(&config.output_dir);
                if out.exists() {
                    if let Err(e) = std::fs::remove_dir_all(out) {
                        eprintln!("error removing {}: {}", out.display(), e);
                        process::exit(1);
                    }
                    eprintln!("removed {}", out.display());
                    cleaned = true;
                }
            }
        }
    }

    if !cleaned {
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

fn cmd_build_from_config_resolved(
    config: &verso_doc::config::ResolvedConfig,
    output_override: Option<&str>,
) {
    if config.output_dir != "." {
        std::fs::create_dir_all(&config.output_dir).unwrap_or_else(|e| {
            eprintln!("error creating {}: {}", config.output_dir, e);
            process::exit(1);
        });
    }

    for paper in &config.papers {
        let output = match output_override {
            Some(o) => o.to_string(),
            None => format!("{}/{}.pdf", config.output_dir, paper.output),
        };
        cmd_build(&paper.input, Some(&output));
    }
}

fn cmd_build(file: &str, output: Option<&str>) {
    use std::process::Command;
    use verso_doc::compile_tex::compile_to_tex;
    use verso_doc::parse::parse_document_from_file;

    let path = Path::new(file);

    // Determine output path and format
    let output_path = match output {
        Some(o) => std::path::PathBuf::from(o),
        None => {
            let stem = path.file_stem().unwrap_or_default().to_string_lossy();
            path.parent()
                .unwrap_or(Path::new("."))
                .join(format!("{}.pdf", stem))
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

struct WatchTask {
    /// Canonical paths this task depends on (the input file + all its !includes)
    deps: Vec<std::path::PathBuf>,
    /// Callback to run when a dependency changes
    run: Box<dyn Fn()>,
}

impl WatchTask {
    fn new<F: Fn() + 'static>(input: &str, run: F) -> Self {
        use verso_doc::parse::collect_dependencies;
        let deps = collect_dependencies(Path::new(input)).unwrap_or_else(|e| {
            eprintln!("warning: cannot resolve dependencies for {}: {}", input, e);
            Path::new(input)
                .canonicalize()
                .map(|p| vec![p])
                .unwrap_or_default()
        });
        WatchTask {
            deps,
            run: Box::new(run),
        }
    }
}

fn watch_and_run(tasks: Vec<WatchTask>) {
    use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
    use std::collections::HashSet;
    use std::sync::mpsc;
    use std::time::Duration;

    // Initial run of all tasks
    print!("\x1b[2J\x1b[H");
    for task in &tasks {
        (task.run)();
    }
    println!("\n\x1b[32mWatching for changes... (Ctrl+C to stop)\x1b[0m");

    let (tx, rx) = mpsc::channel();
    let mut debouncer =
        new_debouncer(Duration::from_millis(300), tx).expect("failed to create file watcher");

    // Watch parent directories of all dependencies
    let mut watched = HashSet::new();
    for task in &tasks {
        for dep in &task.deps {
            let dir = dep.parent().unwrap_or(Path::new(".")).to_path_buf();
            if watched.insert(dir.clone()) {
                debouncer
                    .watcher()
                    .watch(&dir, notify::RecursiveMode::Recursive)
                    .unwrap_or_else(|e| {
                        eprintln!("warning: cannot watch {}: {}", dir.display(), e);
                    });
            }
        }
    }

    let dominated_extensions: &[&str] = &["verso", "bib"];

    loop {
        match rx.recv() {
            Ok(Ok(events)) => {
                let changed: Vec<std::path::PathBuf> = events
                    .iter()
                    .filter(|e| {
                        matches!(e.kind, DebouncedEventKind::Any)
                            && e.path.extension().map_or(false, |ext| {
                                dominated_extensions.iter().any(|de| ext == *de)
                            })
                    })
                    .filter_map(|e| e.path.canonicalize().ok())
                    .collect();

                if changed.is_empty() {
                    continue;
                }

                print!("\x1b[2J\x1b[H");
                let mut any_ran = false;
                for task in &tasks {
                    if changed.iter().any(|c| task.deps.contains(c)) {
                        (task.run)();
                        any_ran = true;
                    }
                }
                if any_ran {
                    println!("\n\x1b[32mWatching for changes... (Ctrl+C to stop)\x1b[0m");
                }
            }
            Ok(Err(e)) => eprintln!("watch error: {}", e),
            Err(_) => break,
        }
    }
}

#[tokio::main]
async fn cmd_lsp() {
    use std::path::PathBuf;
    use tower_lsp::jsonrpc::Result;
    use tower_lsp::lsp_types::*;
    use tower_lsp::{Client, LanguageServer, LspService, Server};
    use verso_doc::compile_tex::find_unresolved_refs_against;
    use verso_doc::dim::DimOutcome;
    use verso_doc::parse::{parse_document, parse_document_from_file};
    use verso_doc::verify::{verify_document, Outcome};

    struct VersoServer {
        client: Client,
        documents: std::sync::RwLock<std::collections::HashMap<Url, String>>,
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
                    completion_provider: Some(CompletionOptions {
                        trigger_characters: Some(vec![":".to_string()]),
                        ..Default::default()
                    }),
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
            let uri = params.text_document.uri.clone();
            let text = params.text_document.text.clone();
            self.documents
                .write()
                .unwrap()
                .insert(uri, text);
            let file_path = params.text_document.uri.to_file_path().ok();
            let diagnostics = compute_diagnostics(&params.text_document.text, file_path.as_deref());
            self.client
                .publish_diagnostics(params.text_document.uri, diagnostics, None)
                .await;
        }

        async fn did_change(&self, params: DidChangeTextDocumentParams) {
            if let Some(change) = params.content_changes.into_iter().last() {
                self.documents
                    .write()
                    .unwrap()
                    .insert(params.text_document.uri.clone(), change.text.clone());
                let diagnostics = compute_diagnostics(&change.text, None);
                self.client
                    .publish_diagnostics(params.text_document.uri, diagnostics, None)
                    .await;
            }
        }

        async fn completion(
            &self,
            params: CompletionParams,
        ) -> Result<Option<CompletionResponse>> {
            let pos = params.text_document_position.position;
            let uri = &params.text_document_position.text_document.uri;

            // Find the `:` that started this completion trigger by reading the document
            let docs = self.documents.read().unwrap();
            let Some(text) = docs.get(uri) else {
                return Ok(None);
            };

            let line_text = text.lines().nth(pos.line as usize).unwrap_or("");
            let col = pos.character as usize;
            let before_cursor = &line_text[..col.min(line_text.len())];

            // Scan backwards from cursor to find the opening `:`
            let colon_col = match before_cursor.rfind(':') {
                Some(idx) => idx,
                None => return Ok(None),
            };

            // The prefix typed so far (between `:` and cursor)
            let typed_prefix = &before_cursor[colon_col + 1..];

            // Check if there's a closing `:` right at the cursor
            let has_closing_colon = line_text.get(col..col + 1) == Some(":");
            let end_character = if has_closing_colon {
                pos.character + 1
            } else {
                pos.character
            };

            let replace_range = Range {
                start: Position {
                    line: pos.line,
                    character: colon_col as u32,
                },
                end: Position {
                    line: pos.line,
                    character: end_character,
                },
            };

            let items: Vec<CompletionItem> = verso_symbolic::unicode::completions(typed_prefix)
                .into_iter()
                .map(|(name, ch)| {
                    let label = format!(":{}:", name);
                    CompletionItem {
                        label: label.clone(),
                        kind: Some(CompletionItemKind::TEXT),
                        detail: Some(format!("{} (U+{:04X})", ch, ch as u32)),
                        filter_text: Some(format!(":{}:", name)),
                        text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                            range: replace_range,
                            new_text: ch.to_string(),
                        })),
                        ..Default::default()
                    }
                })
                .collect();
            Ok(Some(CompletionResponse::Array(items)))
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
            let ref_doc =
                find_root_document(path).and_then(|root| parse_document_from_file(&root).ok());
            let check_doc = ref_doc.as_ref().unwrap_or(&doc);
            for label in find_unresolved_refs_against(check_doc, &doc) {
                let line = text
                    .lines()
                    .enumerate()
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
            end: Position {
                line,
                character: u32::MAX,
            },
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
    let (service, socket) = LspService::new(|client| VersoServer {
        client,
        documents: std::sync::RwLock::new(std::collections::HashMap::new()),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}
