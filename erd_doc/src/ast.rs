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
    /// Prose paragraph.
    Prose(String),
    /// A claim asserting that `lhs` equals `rhs`.
    Claim(Claim),
}

/// An assertion that two expressions are equal.
#[derive(Debug)]
pub struct Claim {
    pub name: String,
    pub lhs: Expr,
    pub rhs: Expr,
    pub span: Span,
}

/// Source location for error reporting.
#[derive(Debug, Clone, Copy)]
pub struct Span {
    pub line: usize,
}
