//! Public API for the document-facing parts of Verso.
//!
//! The root exports are the intended high-level entry points for callers:
//!
//! - parse `.verso` source into an AST
//! - verify claims and proofs
//! - compile verified documents to LaTeX
//! - load and resolve project configuration
//!
//! The individual modules remain available for now, but the crate root is the
//! preferred place to start when integrating `verso_doc`.

pub mod ast;
pub mod compile_tex;
pub mod config;
pub mod dim;
pub mod eval;
pub mod parse;
pub mod report;
mod source;
mod tex_preamble;
mod tex_prose;
mod tex_queries;
pub mod verify;

pub use ast::{
    Block, Claim, ColumnAlign, DefDecl, Document, EnvKind, Environment, ExpectFailType, Figure,
    FuncDecl, List, ListItem, MathBlock, Proof, ProofStep, ProseFragment, Span, Table, VarDecl,
};
pub use compile_tex::{
    collect_labels, collect_symbols, compile_to_tex, find_claim_line, find_decl_line,
    find_label_line, find_symbol, find_unresolved_refs, find_unresolved_refs_against, slugify,
    SymbolInfo,
};
pub use config::{
    default_config_content, find_config, install_schema, load_config, resolve_config,
    stamp_config, strip_jsonc_comments, ConfigError, PaperConfig, ResolvedConfig, ResolvedPaper,
    TestConfig, VersoConfig, CONFIG_FILENAME, CONFIG_FILENAMES, SCHEMA_CONTENT, SCHEMA_FILENAME,
    SCHEMA_REF, VERSION,
};
pub use dim::{check_claim_dim, check_dim, collect_units, DimEnv, DimError, DimOutcome};
pub use eval::{eval_f64, free_vars, spot_check, SpotCheckFailure};
pub use parse::{
    collect_dependencies, parse_document, parse_document_from_file, parse_prose_fragments,
    prose_to_string, resolve_includes, ParseDocError,
};
pub use report::ReportFormatter;
pub use verify::{verify_document, Outcome, VerificationReport, VerificationResult};
