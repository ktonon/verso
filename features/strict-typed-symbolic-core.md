# Strictly Typed Symbolic Core

## Goal

Make `verso_symbolic` strictly typed with respect to physical dimensions and type information. The symbolic core should distinguish:
- dimensionless values, such as `4.5` with type `[1]`
- explicitly typed values, such as `4.5 [m]` with type `[L]`
- unresolved symbolic types, such as a free variable `x` before enough declarations or constraints are available

The core design target is that "missing type information" is no longer modeled as absence. It must be represented explicitly in the IR so that rewrites, simplification, equality checks, tokenization, and ML tooling cannot silently erase or reinterpret type state.

## Plan

### Problem statement

Today the symbolic core is only partially typed:
- `ExprKind::Var { name, indices, dim: Option<Dimension> }` uses `None` to mean both "dimensionless" and "type not yet known"
- Numeric literals (`Rational`, `FracPi`, `Named`) carry no type in the AST; `check_dim` treats them as `[1]` at runtime
- `Quantity(inner, Unit)` stores unit information, but tokenization drops it (`token.rs:173`: "unit info is lost") and `expr_to_pattern` strips it when converting claims to rewrite rules
- `UndeclaredVar` is a runtime error path used to represent "typeless" — not a first-class type state

This causes three architectural problems:
1. The AST conflates "dimensionless" and "type not known yet" — both are `dim: None`
2. Type information is lost at three boundaries: `Pattern::substitute` (always sets `dim: None`), `tokenize`/`detokenize` (drops units and dims), and `expr_to_pattern` (strips Quantity wrappers)
3. Type validity is enforced late by `check_dim` rather than by the shape of the core IR

### Current architecture

After the span-errors feature, `Expr` is a struct:

```rust
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}
```

`ExprKind` is the expression enum with variants: `Rational`, `FracPi`, `Named`, `Var { name, indices, dim }`, `Add`, `Mul`, `Neg`, `Inv`, `Pow`, `Fn`, `FnN`, `Quantity(inner, Unit)`.

Type-related state is currently stored in three places:
- **Inline on Var**: `dim: Option<Dimension>` — set by parser when `[L T^-1]` follows a variable
- **In Quantity nodes**: `Unit` carries `dimension: Dimension` and `scale: f64`
- **In Context.dims**: `DimEnv = HashMap<String, Dimension>` — populated by `:var` declarations

`check_dim` (context.rs) walks the expression tree bottom-up, checking Var inline dims first, then falling back to `DimEnv` lookups. It returns `DimError` with `Span` for error reporting.

### Target model

Add an explicit `Ty` field to `Expr`, extending the existing struct:

```rust
pub enum Ty {
    /// Resolved physical dimension: [1], [L], [M L T^-2], etc.
    Concrete(Dimension),
    /// Unresolved — type is not yet known. Not dimensionless.
    Unresolved,
}

pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
    pub ty: Ty,
}
```

This avoids duplicating the AST into separate SurfaceExpr/TypedExpr types. The parser produces expressions with `Ty::Unresolved` by default, then an elaboration pass fills in concrete types using the same information `check_dim` uses today.

Key invariants after elaboration:
- Every numeric literal (`Rational`, `FracPi`, `Named`) has `Ty::Concrete([1])`
- Every `Quantity` node has `Ty::Concrete(unit.dimension)`
- Every `Var` with a declaration or inline annotation has `Ty::Concrete(dim)`
- Undeclared variables remain `Ty::Unresolved`
- Rewrites preserve `Ty`
- Tokenization and ML-facing serialization either preserve `Ty` or operate on a clearly separate untyped projection with an explicit boundary

### Core design

#### 1. Extend Expr with Ty

Since `Expr` is already a struct (from the span-errors refactor), adding `ty: Ty` is a field addition — the same migration pattern as adding `span: Span`. Every `Expr::new(kind)` call sets `ty: Ty::Unresolved`. Every `Expr::spanned(kind, span)` call also sets `ty: Ty::Unresolved`.

The parser continues to produce `Ty::Unresolved` expressions. A separate elaboration pass resolves types.

#### 2. Elaboration pass

Add `elaborate(expr: &Expr, env: &DimEnv) -> Expr` that walks the tree and fills in `Ty`:

- `Rational`, `FracPi`, `Named` → `Ty::Concrete(Dimension::dimensionless())`
- `Quantity(_, unit)` → `Ty::Concrete(unit.dimension.clone())`
- `Var { dim: Some(d), .. }` → `Ty::Concrete(d.clone())`
- `Var { dim: None, name }` where `env.contains(name)` → `Ty::Concrete(env[name].clone())`
- `Var { dim: None, name }` where `!env.contains(name)` → `Ty::Unresolved`
- `Add(a, b)` → propagate from children (must match)
- `Mul(a, b)` → multiply dimensions
- `Pow(base, exp)` → base dimension raised to integer exponent
- `Fn(_, arg)` → `Ty::Concrete([1])` (trig/log require dimensionless args)
- `Neg(inner)` / `Inv(inner)` → propagate from child

This is essentially `check_dim` rewritten to attach results rather than just validate.

#### 3. Preserve type information through rewrites

`Pattern::substitute` (rule.rs) currently hardcodes `dim: None` when reconstructing `Var` nodes. Fix this by:
- Storing the matched expression's `Ty` in `Bindings` alongside the structural match
- Restoring `Ty` during substitution

For `Quantity` in `expr_to_pattern` (context.rs), which currently strips the unit wrapper:
- Either preserve unit information in the pattern, or
- Re-elaborate after substitution to restore types

Preferred direction: keep rule patterns mostly structural, but carry `Ty` through bindings so substitution preserves it. Add a typed post-check that rejects ill-typed rewrites.

#### 4. Define a deliberate boundary for ML/token tooling

Current tokenization (`token.rs`) explicitly drops units and dims:
- `Quantity(inner, _unit)` → tokenizes only `inner`
- `Var { name, indices, .. }` → tokenizes as De Bruijn index, no dim
- `detokenize` → reconstructs with `dim: None`

Two viable options:
- **Typed tokenization**: add `Ty` tokens to the vocabulary, making ML artifacts type-aware
- **Explicit untyped projection**: create `strip_types(expr) -> Expr` that sets all `ty` to `Unresolved` and unwraps `Quantity` nodes, used only at the ML boundary

The second path is lower-risk initially. The conversion must be explicit and documented as lossy.

### Phased migration

#### Phase 1 — Ty field and elaboration

- Add `Ty` enum to `expr.rs`
- Add `ty: Ty` field to `Expr` struct
- Update `Expr::new` and `Expr::spanned` to default to `Ty::Unresolved`
- Update all `ExprKind` match sites (same migration pattern as span-errors Phase 1 — `PartialEq` ignores `ty`, just like it ignores `span`)
- Add `elaborate(expr, env) -> Expr` in `context.rs`
- Wire elaboration into `Context` methods so expressions are elaborated after parsing

This is the heaviest phase (touches every Expr construction site), but the span-errors refactor established the pattern.

#### Phase 2 — Replace check_dim with Ty-based validation

- Rewrite `check_dim` to read `ty` fields rather than recomputing dimensions bottom-up
- Or: keep `check_dim` as a validator that confirms `ty` fields are consistent (simpler migration)
- `check_expr_dim`, `check_claim_dim`, and `check_dims` consume `Ty` instead of re-inferring
- DimError continues to carry spans for underline display
- `infer_type` reads `ty` directly instead of running check_dim

#### Phase 3 — Rewrite/search preservation

- Update `Bindings` (rule.rs) to store `Ty` for matched wildcards
- Update `Pattern::substitute` to restore `Ty` on reconstructed nodes (especially `Var`)
- Update `expr_to_pattern` (context.rs) to preserve `Quantity` type information in claim-derived rules
- Audit `eval_constants`, `simplify`, `all_rewrites_depth` in `search.rs` for `Ty` preservation
- Add tests: rewrite `x + 0 → x` preserves `Ty`, `Quantity` survives simplification

#### Phase 4 — Token and ML boundary

- Add explicit `strip_types(expr) -> Expr` projection
- Update `tokenize`/`detokenize` to use the projection (making type loss explicit)
- Or: add type tokens to vocabulary if typed ML training is desired
- Update training-data validation to document the typed/untyped boundary

#### Phase 5 — Consumer integration

- REPL: display `Ty` alongside results (already partially done via `infer_type`/`format_type_suffix`)
- `verso_doc` verification: `verify_claim`/`verify_proof` consume elaborated expressions
- LSP diagnostics: use `Ty` for hover-over type display
- Regression tests for declarations, claims, proof steps, and unit-bearing constants

### Key files

| File | Current state | Changes needed |
|------|--------------|----------------|
| `verso_symbolic/src/expr.rs` | `Expr { kind, span }`, `ExprKind::Var { dim: Option<Dimension> }` | Add `ty: Ty` field, `Ty` enum, update `PartialEq` to ignore `ty` |
| `verso_symbolic/src/context.rs` | `check_dim` walks tree bottom-up, `DimEnv` stores declared dims, `DimError` carries `Span` | Add `elaborate()`, refactor `check_dim` to validate `Ty` fields |
| `verso_symbolic/src/parser.rs` | Attaches inline dims to Var, wraps numeric+unit as Quantity, enforces dim-on-vars/unit-on-numbers constraint | No changes needed — parser continues producing `Ty::Unresolved`; elaboration is separate |
| `verso_symbolic/src/rule.rs` | `Pattern::substitute` hardcodes `dim: None` on Var reconstruction | Store and restore `Ty` through `Bindings` |
| `verso_symbolic/src/search.rs` | `eval_constants`, `simplify`, `all_rewrites_depth` rebuild expressions without type consideration | Propagate `Ty` through expression reconstruction |
| `verso_symbolic/src/token.rs` | `tokenize` drops Quantity units, `detokenize` sets `dim: None` | Use explicit `strip_types` projection; document as lossy |
| `verso_symbolic/src/training_data.rs` | ML vocabulary is untyped | Document untyped boundary; optionally add type tokens |
| `verso_doc/src/verify.rs` | Calls `check_dims` for claim verification | Consume elaborated expressions with `Ty` |

## Implementation Notes

### Current type-loss points (verified)

1. **Pattern::substitute** (rule.rs) — reconstructs `Var` with `dim: None` always
2. **tokenize** (token.rs:173) — `Quantity(inner, _unit)` drops unit, comment says "unit info is lost"
3. **detokenize** (token.rs:254) — reconstructs `Var` with `dim: None` always
4. **expr_to_pattern** (context.rs) — strips `Quantity` wrapper: `Quantity(inner, _unit) => expr_to_pattern(inner, wildcards)`
5. **substitute_consts** (context.rs) — reconstructs `Quantity` preserving unit, but doesn't propagate dim to inner Var nodes

### Relationship to span-errors feature

The span-errors feature (completed) established the pattern for extending `Expr` with metadata:
- `Expr` became a struct wrapping `ExprKind` + `Span`
- `PartialEq` compares only `kind` (ignores `span`)
- Every construction site was updated in a single migration phase

Adding `ty: Ty` follows the same pattern. `PartialEq` will ignore both `span` and `ty`, comparing only structural `kind`. The span-errors migration touched 16 files; the `ty` migration will touch the same files.

### Implemented in this session

Phase 1 is now started in code:
- `Expr` has a `ty: Ty` field in `expr.rs`
- `Ty` is explicit: `Concrete(Dimension)` or `Unresolved`
- `Expr::new` and `Expr::spanned` default to `Ty::Unresolved`
- Added `Expr::typed` / `Expr::spanned_typed` helpers for metadata-preserving construction
- Added `Context::elaborate_expr(&self, expr) -> Result<Expr, DimError>`
- `infer_type` now reads the elaborated root `ty` instead of recomputing with `check_dim`

The elaboration pass currently covers:
- literals -> `Concrete([1])`
- quantities -> `Concrete(unit.dimension)`
- declared or inline-dimension variables -> `Concrete(dim)`
- undeclared variables -> `Unresolved`
- arithmetic/function nodes -> either a derived `Concrete(...)`, `Unresolved`, or the same `DimError` that the legacy checker would have reported for obviously ill-typed expressions

Phase 2 is also now in place:
- `check_dim` elaborates first, then validates typed expressions instead of inferring directly from `DimEnv`
- typed expressions can now be dimension-checked again without requiring the original environment for already-resolved variables
- `check_expr_dim` and `check_dims` therefore consume the typed elaboration path through `check_dim`

Phase 3 is now in place:
- added `Expr::derived(...)` / `Expr::spanned_derived(...)` and `infer_ty_from_kind(...)` in `expr.rs`
- `Pattern::substitute` now reconstructs typed nodes instead of defaulting to `Ty::Unresolved`
- wildcard-driven `Var` substitution preserves the bound variable's `dim` and `ty`
- claim-derived rewrite rules preserve `Quantity` wrappers instead of stripping unit-bearing nodes
- `search.rs` rebuild sites and constant-folding now derive `ty` from their typed children instead of erasing it
- `Context::simplify`, `check_equal`, and `exprs_equivalent` feed elaborated expressions into the rewrite pipeline when elaboration succeeds

This is intentionally not the full migration yet. Tokenization and ML serialization still do not preserve `ty` end-to-end, and the consumer-facing integrations are still incomplete.

### Open questions

- Should `Ty` use `Unresolved` (simple) or `Symbol(TypeVarId)` (supports unification)? Start with `Unresolved`; add type variables only if unification is needed later.
- Should user-defined functions (`:func`) carry typed signatures? Not initially — elaborate the body at call sites.
- Should the ML subsystem remain intentionally untyped? Yes initially — use an explicit untyped projection with `strip_types`.

### Recommended decisions

- Extend `Expr` with `ty: Ty` rather than introducing a separate `TypedExpr` AST — the struct is already extensible and the migration pattern is proven
- Treat dimensionless as `Ty::Concrete(Dimension::dimensionless())`, never as "missing"
- Treat unresolved variables as `Ty::Unresolved`, never as `None`
- Make every lossy typed-to-untyped conversion explicit in the API

## Verification

Automated:

```bash
cargo test -p verso_symbolic
cargo test -p verso_doc
npm test
```

Expected regression coverage:
- literals elaborate to `Concrete([1])`
- quantities elaborate to concrete physical dimensions
- undeclared variables elaborate to `Unresolved`, not missing metadata
- typed rewrites preserve type state
- claim-derived rules do not erase quantity wrappers, units, or inline dimensions
- tokenization either preserves type state or uses an explicitly lossy projection
- REPL displays consistent types for `4.5`, `4.5 [m]`, declared vars, and consts with units

Manual checks:
- In the REPL, `4.5` reports `[1]`
- In the REPL, `4.5 [m]` reports `[L]`
- A bare variable with no declaration is shown as having an unresolved type, not as silently dimensionless
- Ill-typed equalities are rejected before symbolic or numerical equivalence fallback

Completed this session:
- `cargo test -p verso_symbolic`
- `cargo test -p verso_doc`

New automated coverage added:
- elaboration marks plain literals as `Ty::Concrete([1])`
- elaboration marks quantities with the unit dimension
- elaboration keeps undeclared variables `Ty::Unresolved`
- elaboration uses declared dimensions to type variables and typed addition
- `check_dim` accepts an already-elaborated expression without needing the original `DimEnv`
- variable-pattern substitution preserves `dim` and `ty`
- simplification preserves a concrete type through rule application for a typed expression like `x * 1`
