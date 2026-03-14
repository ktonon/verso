use verso_doc::compile_tex::find_unresolved_refs_against;
use verso_doc::dim::DimOutcome;
use verso_doc::parse::{parse_document, parse_document_from_file};
use verso_doc::verify::{verify_document, Outcome};
use std::path::{Path, PathBuf};
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
            // Fast path: skip cross-file ref resolution on every keystroke
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

        // Dimension diagnostics
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

    // Unresolved ref diagnostics — only when file_path is provided (open/save, not change)
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

/// Convert a 1-based line number to an LSP Range spanning the entire line.
fn line_range(line: usize) -> Range {
    let line = (line.saturating_sub(1)) as u32;
    Range {
        start: Position {
            line,
            character: 0,
        },
        end: Position {
            line,
            character: u32::MAX,
        },
    }
}

/// Walk up from a file to find the root `.verso` document.
///
/// Searches for `paper.verso` or `index.verso` in ancestor directories.
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

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| VersoServer { client });
    Server::new(stdin, stdout, socket).serve(service).await;
}
