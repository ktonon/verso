use crate::dim::Dimension;
use erd_symbolic::Expr;

/// A parsed `.erd` document.
#[derive(Debug)]
pub struct Document {
    pub blocks: Vec<Block>,
}

/// A top-level block in an `.erd` document.
#[derive(Debug)]
pub enum Block {
    /// Section heading (level 1 = `#`, level 2 = `##`, etc.)
    Section { level: u8, title: String, span: Span },
    /// Prose paragraph (may contain inline tagged expressions).
    Prose(Vec<ProseFragment>),
    /// A claim asserting that `lhs` equals `rhs`.
    Claim(Claim),
    /// A proof chain for a named claim.
    Proof(Proof),
    /// A dimension declaration for a variable.
    Dim(DimDecl),
}

/// A fragment within a prose paragraph.
#[derive(Debug, Clone)]
pub enum ProseFragment {
    /// Plain text.
    Text(String),
    /// Tagged inline math: math`expr` — parsed via erd_symbolic, rendered via ToTex.
    Math(Expr),
    /// Tagged inline raw LaTeX: tex`\vec{v}` — passed through as-is.
    Tex(String),
    /// Tagged claim reference: claim`name` — rendered as \eqref{eq:name}.
    ClaimRef(String),
}

/// An assertion that two expressions are equal.
#[derive(Debug)]
pub struct Claim {
    pub name: String,
    pub lhs: Expr,
    pub rhs: Expr,
    pub span: Span,
}

/// A step-by-step proof for a named claim.
#[derive(Debug)]
pub struct Proof {
    pub claim_name: String,
    pub steps: Vec<ProofStep>,
    pub span: Span,
}

/// A single step in a proof chain.
#[derive(Debug)]
pub struct ProofStep {
    pub expr: Expr,
    /// Optional justification (rule name) after `;`.
    pub justification: Option<String>,
    pub span: Span,
}

/// A dimension declaration: `:dim varname [M L T^-2]`
#[derive(Debug)]
pub struct DimDecl {
    pub var_name: String,
    pub dimension: Dimension,
    pub span: Span,
}

/// Source location for error reporting.
#[derive(Debug, Clone, Copy)]
pub struct Span {
    pub line: usize,
}
