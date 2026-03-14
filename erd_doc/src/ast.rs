use erd_symbolic::{Dimension, Expr};

/// A parsed `.erd` document.
#[derive(Debug)]
pub struct Document {
    pub blocks: Vec<Block>,
}

/// A top-level block in an `.erd` document.
#[derive(Debug)]
pub enum Block {
    /// Section heading (level 1 = `#`, level 2 = `##`, etc.)
    Section { level: u8, title: String, label: Option<String>, span: Span },
    /// Prose paragraph (may contain inline tagged expressions).
    Prose(Vec<ProseFragment>),
    /// A claim asserting that `lhs` equals `rhs`.
    Claim(Claim),
    /// A proof chain for a named claim.
    Proof(Proof),
    /// A dimension declaration for a variable.
    Dim(DimDecl),
    /// A list (bullet or numbered).
    List(List),
    /// A displayed math block (not verified).
    MathBlock(MathBlock),
    /// A bibliography declaration: `:bibliography refs.bib`
    Bibliography { path: String, span: Span },
    /// A theorem-like environment: `:theorem`, `:definition`, etc.
    Environment(Environment),
    /// A block quote: lines starting with `> `.
    BlockQuote(Vec<ProseFragment>),
    /// Document title: `:title text`
    Title(String),
    /// Document author: `:author name`
    Author(String),
    /// Document date: `:date text`
    Date(String),
    /// Document abstract: `:abstract` with indented body
    Abstract(Vec<ProseFragment>),
    /// A figure: `:figure path` with optional caption, label, width
    Figure(Figure),
    /// A table: `:table Title` with pipe-delimited rows
    Table(Table),
    /// Centered block: `:center` with indented body
    Center(Vec<ProseFragment>),
    /// Page break: `:pagebreak`
    PageBreak,
    /// Table of contents: `:toc`
    Toc,
}

/// A figure block.
#[derive(Debug)]
pub struct Figure {
    pub path: String,
    pub caption: Option<Vec<ProseFragment>>,
    pub label: Option<String>,
    pub width: f64,
    pub span: Span,
}

/// A table block.
#[derive(Debug)]
pub struct Table {
    pub title: Option<String>,
    pub columns: Vec<ColumnAlign>,
    pub header: Vec<Vec<ProseFragment>>,
    pub rows: Vec<Vec<Vec<ProseFragment>>>,
    pub label: Option<String>,
    pub span: Span,
}

/// Column alignment for tables.
#[derive(Debug, Clone, Copy)]
pub enum ColumnAlign {
    Left,
    Center,
    Right,
}

/// A theorem-like environment block.
#[derive(Debug)]
pub struct Environment {
    pub kind: EnvKind,
    pub title: Option<String>,
    pub body: Vec<ProseFragment>,
    pub span: Span,
}

/// The kind of theorem-like environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EnvKind {
    Theorem,
    Lemma,
    Definition,
    Corollary,
    Remark,
    Example,
}

/// A fenced math block: ```math ... ```
#[derive(Debug)]
pub struct MathBlock {
    pub exprs: Vec<Expr>,
    pub span: Span,
}

/// A list block (ordered or unordered).
#[derive(Debug)]
pub struct List {
    pub ordered: bool,
    pub items: Vec<ListItem>,
    pub span: Span,
}

/// An item within a list.
#[derive(Debug)]
pub struct ListItem {
    pub fragments: Vec<ProseFragment>,
    pub children: Option<List>,
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
    /// Bold text: **text** — rendered as \textbf{text}.
    Bold(Vec<ProseFragment>),
    /// Italic text: *text* — rendered as \textit{text}.
    Italic(Vec<ProseFragment>),
    /// Citation: cite`key` or cite`key1,key2` — rendered as \cite{keys}.
    Cite(Vec<String>),
    /// Footnote: ^[text] — rendered as \footnote{text}.
    Footnote(Vec<ProseFragment>),
    /// Cross-reference: ref`label` or ref`label|display text` — rendered as \hyperref.
    Ref {
        label: String,
        display: Option<String>,
    },
    /// URL: url`https://...` or url`https://...|display text` — rendered as \url or \href.
    Url {
        url: String,
        display: Option<String>,
    },
    /// Paragraph break within an indented block (blank line between indented lines).
    ParBreak,
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
