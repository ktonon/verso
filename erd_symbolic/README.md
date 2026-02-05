## erd_symbolic
## Rust Library Outline

ERD stands for emergent rung dynamics. It is a provisional implementation of emergent rung model space.

This library is a support to tool for developing ERD.

Purpose
A symbolic mathematics library in Rust for the ERD project, designed to:

1.	Represent mathematical expressions as executable, testable ASTs
2.	Render expressions to LaTeX for paper writing
3.	Support tensor algebra from the ground up
4.	Compile to efficient numeric evaluation for simulations

### Goals

The library serves three interconnected purposes:

**Executable mathematics.** Expressions are represented as an AST that can be evaluated, differentiated, and simplified. Unit tests validate algebraic identities and transformations, ensuring the mathematics is correct before it appears in any paper or simulation.

**Document generation.** The same AST that executes also renders to LaTeX. A `ToTex` trait produces publication-ready output, with smart rendering that displays `Add(a, Neg(b))` as subtraction. The paper and the tests share a single source of truth.

**Simulation compilation.** For numerical work, symbolic expressions compile to efficient executable forms. The library distinguishes between symbolic manipulation (flexible, expressive) and numeric evaluation (tight loops, GPU-friendly). Parameters fixed for a simulation run can be precomputed; only true inputs vary per evaluation.

### Design Principles

**Tensors from the start.** Rather than building scalar algebra and retrofitting tensors later, the AST represents indexed quantities natively. Scalars are simply rank-0 tensors. This prepares the library for continuum mechanics where stress, strain, and deformation are fundamental.

**Minimal canonical forms.** Subtraction and division are not AST variants—they're represented as addition with negation and multiplication with inversion. This reduces the number of patterns in simplification rules while `ToTex` still renders legible output.

**Rewriting as search.** A rule defines an equivalence between two expression patterns. Neither side is inherently simpler—`a * (b + c)` and `(a * b) + (a * c)` are just different forms, each preferable in different contexts. Simplification is search through the space of equivalent expressions. A rule can be applied in either direction. The complexity measure evaluates the result after application to guide the search toward lower-complexity forms overall. A transformation that increases complexity locally may enable further transformations that reduce it globally.

**Extensible rule sets.** The library provides standard algebraic identities but does not impose them. Client libraries define domain-specific rules and combine them as needed. The rewrite engine is agnostic about which rules it applies.

---

## Core Types

### Expression AST

```rust
pub enum Expr {
    Const(f64),
    Var { name: String, indices: Vec<Index> },
    Add(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Neg(Box<Expr>),
    Inv(Box<Expr>),
    Pow(Box<Expr>, Box<Expr>),
    Fn(FnKind, Box<Expr>),
}

pub struct Index {
    pub name: String,
    pub position: IndexPosition,
}

pub enum IndexPosition {
    Upper,   // contravariant
    Lower,   // covariant
}

pub enum FnKind {
    Sin, Cos, Exp, Ln,
    // extend as needed
}
```

### Design Decisions

- Scalars are rank-0 tensors (empty indices vec)
- No `Sub` or `Div` variants; represented as `Add(a, Neg(b))` and `Mul(a, Inv(b))`
- `ToTex` renders these as subtraction/division for legibility
- Einstein summation: matching index names with opposite positions contract

---

## Builder Functions

```rust
pub fn constant(n: f64) -> Expr
pub fn scalar(name: &str) -> Expr
pub fn tensor(name: &str, indices: Vec<Index>) -> Expr
pub fn upper(name: &str) -> Index
pub fn lower(name: &str) -> Index
pub fn add(a: Expr, b: Expr) -> Expr
pub fn mul(a: Expr, b: Expr) -> Expr
pub fn neg(a: Expr) -> Expr
pub fn inv(a: Expr) -> Expr
pub fn pow(base: Expr, exp: Expr) -> Expr
pub fn sin(a: Expr) -> Expr
pub fn cos(a: Expr) -> Expr
pub fn exp(a: Expr) -> Expr
pub fn ln(a: Expr) -> Expr
```

---

## Traits

### ToTex

```rust
pub trait ToTex {
    fn to_tex(&self) -> String;
}
```

- All AST nodes implement `ToTex`
- Rendering is smart: `Add(a, Neg(b))` renders as `a - b`
- Same AST serves symbolic manipulation and document generation

---

## Operations

### Substitution

```rust
impl Expr {
    pub fn substitute(&self, bindings: &HashMap<String, Expr>) -> Expr;
}
```

- Replace variables with other expressions
- Foundation for evaluation

### Simplification

```rust
impl Expr {
    pub fn simplify(&self) -> Expr;
    pub fn complexity(&self) -> usize;
}
```

- Reduce constant operations
- Apply algebraic identities
- Complexity measure guides simplification search

### Evaluation

```rust
impl Expr {
    pub fn eval(&self, bindings: &HashMap<String, Expr>) -> Expr;
    pub fn try_as_f64(&self) -> Option<f64>;
}
```

- `eval` = substitute + simplify
- Partial evaluation supported (some variables remain symbolic)

---

## Rewrite System

### Patterns

```rust
pub enum Pattern {
    Wildcard(String),
    ConstWild(String),
    Const(f64),
    Add(Box<Pattern>, Box<Pattern>),
    Mul(Box<Pattern>, Box<Pattern>),
    Neg(Box<Pattern>),
    Inv(Box<Pattern>),
    Pow(Box<Pattern>, Box<Pattern>),
    Fn(FnKind, Box<Pattern>),
}

pub struct Rule {
    pub name: String,
    pub lhs: Pattern,
    pub rhs: Pattern,
}

pub type Bindings = HashMap<String, Expr>;
```

### Pattern Operations

```rust
impl Pattern {
    pub fn match_expr(&self, expr: &Expr) -> Option<Bindings>;
    pub fn substitute(&self, bindings: &Bindings) -> Expr;
}

impl Rule {
    pub fn apply_ltr(&self, expr: &Expr) -> Option<Expr>;  // left to right
    pub fn apply_rtl(&self, expr: &Expr) -> Option<Expr>;  // right to left
}
```

### Rule Sets

```rust
pub struct RuleSet {
    rules: Vec<Rule>,
}

impl RuleSet {
    pub fn new() -> Self;
    pub fn add(&mut self, rule: Rule) -> &mut Self;
    pub fn merge(&mut self, other: RuleSet) -> &mut Self;
}

pub fn standard_rules() -> RuleSet;      // arithmetic identities
pub fn trigonometric_rules() -> RuleSet; // sin²x + cos²x = 1, etc.
pub fn tensor_rules() -> RuleSet;        // index contraction, symmetries
```

Client libraries define domain-specific rules and combine them:

```rust
let rules = RuleSet::new()
    .merge(standard_rules())
    .merge(trigonometric_rules())
    .merge(my_domain_rules());
```

### Search Strategy

```rust
pub trait SearchStrategy {
    fn simplify(&self, expr: &Expr, rules: &RuleSet) -> Expr;
}

pub struct GreedySearch {
    pub max_steps: usize,
}

impl SearchStrategy for GreedySearch {
    fn simplify(&self, expr: &Expr, rules: &RuleSet) -> Expr;
}
```

Future strategies (beam search, equality saturation) implement the same trait.

---

## Tensor Semantics

### Contraction

- Multiplication with matching index names (one upper, one lower) contracts
- Free indices determine result rank

### Index Equivalence

- `A^i B_i = A^j B_j` (α-equivalence)
- Canonicalize index names before comparison when needed

### Future Considerations

- Symmetric/antisymmetric tensor properties
- Domain-specific identities (Bianchi, etc.)

---

## Compilation Pipeline (Future)

```
Symbolic Expr
    ↓ simplify, apply identities
Optimized Expr
    ↓ compile
Executable form
    ↓ bind constants
Numeric evaluation
```

### Considerations

- Separate `CompiledExpr` type for execution
- Distinguish parameters (fixed per run) vs inputs (varying per evaluation)
- Target dense/sparse representations, GPU kernels, or `nalgebra` operations
- No allocation in hot path

---

## Dependencies

- None for core symbolic manipulation
- Future: `egg` for equality saturation if greedy rewriting is insufficient
- Future: `nalgebra` or GPU backend for compiled evaluation

---

## Testing Strategy

- Unit tests for Display/ToTex output
- Unit tests for substitution and evaluation
- Unit tests for pattern matching
- Property tests for identity preservation (simplify doesn't change meaning)
- Numeric validation: symbolic differentiation matches finite differences
