use erd_doc::compile_tex::find_unresolved_refs;
use erd_doc::dim::DimOutcome;
use erd_doc::parse::parse_document;
use erd_doc::verify::{verify_document, Outcome};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

struct ErdServer {
    client: Client,
}

#[tower_lsp::async_trait]
impl LanguageServer for ErdServer {
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
            .log_message(MessageType::INFO, "erd-lsp initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.check_document(params.text_document.uri, &params.text_document.text)
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().last() {
            self.check_document(params.text_document.uri, &change.text)
                .await;
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        if let Some(text) = params.text {
            self.check_document(params.text_document.uri, &text).await;
        }
    }
}

impl ErdServer {
    async fn check_document(&self, uri: Url, text: &str) {
        let diagnostics = compute_diagnostics(text);
        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }
}

fn compute_diagnostics(text: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    let doc = match parse_document(text) {
        Ok(d) => d,
        Err(e) => {
            diagnostics.push(Diagnostic {
                range: line_range(e.line),
                severity: Some(DiagnosticSeverity::ERROR),
                message: e.message,
                source: Some("erd".to_string()),
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
                    source: Some("erd".to_string()),
                    ..Default::default()
                });
            }
            Outcome::Fail { residual } => {
                diagnostics.push(Diagnostic {
                    range: line_range(result.span.line),
                    severity: Some(DiagnosticSeverity::ERROR),
                    message: format!("'{}' failed. Residual: {}", result.name, residual),
                    source: Some("erd".to_string()),
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
                    source: Some("erd".to_string()),
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
                        source: Some("erd".to_string()),
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
                        source: Some("erd".to_string()),
                        ..Default::default()
                    });
                }
            }
        }
    }

    // Unresolved ref diagnostics
    for label in find_unresolved_refs(&doc) {
        // Find the line containing this ref for positioning
        let line = text.lines().enumerate()
            .find(|(_, l)| l.contains(&format!("ref`{}", label)))
            .map(|(i, _)| i + 1)
            .unwrap_or(0);
        diagnostics.push(Diagnostic {
            range: line_range(line),
            severity: Some(DiagnosticSeverity::WARNING),
            message: format!("unresolved reference: '{}'", label),
            source: Some("erd".to_string()),
            ..Default::default()
        });
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

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| ErdServer { client });
    Server::new(stdin, stdout, socket).serve(service).await;
}
