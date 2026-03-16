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
- `Expr::Var` stores `dim: Option<Dimension>`
- numeric literals have no attached type in the AST and are treated as dimensionless only during `check_dim`
- `Quantity` stores unit information, but several downstream systems intentionally strip it
- `UndeclaredVar` is used as a runtime error path to represent "typeless" expressions

This causes three architectural problems:
1. The AST conflates "dimensionless" and "type not known yet"
2. Typed information is lost when expressions move through rewrite, token, and training-data boundaries
3. Type validity is enforced late by helper passes instead of by the shape of the core IR

### Target model

Introduce an explicit type layer.

```text
SurfaceExpr
  Parser output, unresolved names, optional annotations

Ty
  Concrete(Dimension)
  Symbol(TypeVarId)

TypedExpr
  Every node carries a Ty
```

Key invariants:
- Every numeric literal is `Concrete([1])`
- Every quantity with a unit is `Concrete(unit.dimension)`
- Every variable has a type at construction time: either `Concrete(...)` or `Symbol(...)`
- Rewrites preserve `Ty`
- Tokenization and ML-facing serialization either preserve `Ty` or operate on a clearly separate untyped projection with an explicit boundary

### Core design

#### 1. Split parsing/binding from typed core

Keep a surface AST for parsing user input and a typed core IR for symbolic manipulation.

Suggested layering:

```text
parser -> SurfaceExpr
binder/type elaboration -> TypedExpr
search/rules/equality -> TypedExpr
ML/token projection -> UntypedExpr or typed token stream
```

The parser should not be responsible for global type resolution. Its job is to preserve source meaning. A later elaboration step should resolve names, attach concrete dimensions, or allocate symbolic type variables.

#### 2. Make type state explicit

Replace `Option<Dimension>` with a first-class type representation.

Suggested starting point:

```rust
pub enum Ty {
    Concrete(Dimension),
    Symbol(TypeVarId),
}
```

Then either:
- add `ty: Ty` to every `TypedExpr` node, or
- use typed wrappers around the existing expression shape if that produces a cleaner migration

Important semantic distinction:
- `[1]` is a real, concrete type
- `Symbol(t0)` is unresolved, not dimensionless

#### 3. Preserve type information through rewrites

Rules and rewrites currently operate on expression shape only. The typed design must define one of these approaches:
- typed rules, where patterns carry type constraints
- untyped structural rules plus a typed post-check that rejects ill-typed rewrites

Preferred direction:
- keep rule syntax mostly structural
- attach typed binding validation during match/substitute
- require rule application to preserve or consistently transform `Ty`

Examples:
- `x + 0 -> x` is valid only when both sides have the same `Ty`
- `x * x -> x^2` must preserve `Ty`
- trig/log rules should require dimensionless arguments
- claim-derived rules must include type constraints from the verified claim

#### 4. Define a deliberate boundary for ML/token tooling

Current tokenization drops units and inline dimensions. That is acceptable only if it is treated as an explicit projection, not as a round-trip representation of the core symbolic term.

Two viable options:
- typed tokenization: include type tokens and make ML artifacts type-aware
- untyped projection: create a separate `UntypedExpr` used only for training/search data generation, with explicit conversion from `TypedExpr`

The second path is lower-risk initially, but the conversion must be explicit and documented as lossy.

### Phased migration

#### Phase 1 — Type model and typed elaboration

- Introduce `Ty`
- Add a binder/elaboration pass from parser output to typed core
- Make literals elaborate to `Concrete([1])`
- Make `:var` declarations elaborate to `Concrete(dim)` when provided, otherwise `Symbol(...)`
- Make `:const` declarations elaborate once and store typed values in `Context`

#### Phase 2 — Typed equality and dimensional analysis

- Replace `check_dim` as the primary source of type truth
- Reframe dimension checking as validation over `TypedExpr`
- Keep diagnostic helpers for user-facing errors, but not as the main type engine
- Make `check_equal` reject or classify ill-typed equalities before numerical fallback

#### Phase 3 — Rewrite/search preservation

- Update `Pattern`, `Bindings`, and substitution to preserve typed variables and quantities
- Ensure claim-derived rules retain type constraints
- Audit simplification helpers for typed correctness

#### Phase 4 — Token and ML boundary cleanup

- Decide on typed tokenization or explicit untyped projection
- Remove any APIs that appear round-trippable while silently dropping types
- Update training-data vocabulary and validation docs accordingly

#### Phase 5 — Consumer integration

- Update repl to display explicit type state
- Update `verso_doc` verification to consume typed expressions
- Add regression tests around declarations, claims, proof steps, and unit-bearing constants

### Key files

- `verso_symbolic/src/expr.rs` — current AST conflates missing and resolved type state
- `verso_symbolic/src/context.rs` — type checking and declaration storage currently depend on runtime dimension inference
- `verso_symbolic/src/parser.rs` — parser currently encodes some typing rules as syntax-time heuristics
- `verso_symbolic/src/rule.rs` — substitution rebuilds vars without preserved type state
- `verso_symbolic/src/search.rs` — simplification assumes `Expr` is safe to rewrite structurally
- `verso_symbolic/src/token.rs` — token round-trip currently drops units and inline dimensions
- `verso_symbolic/src/training_data.rs` — ML vocabulary assumes an untyped expression language
- `verso_doc/src/verify.rs` — document verification should eventually consume typed expressions from the core

## Implementation Notes

### Current issues observed

- `Expr::Var { ..., dim: Option<Dimension> }` models typed state as optional metadata
- `check_dim()` interprets literals as `[1]` and unresolved vars as runtime errors instead of core IR states
- `expr_to_pattern()` strips `Quantity` units when converting claims into rewrite rules
- `Pattern::substitute()` reconstructs `Expr::Var` with `dim: None`
- `tokenize()` serializes `Quantity` by dropping the unit, and `detokenize()` restores vars with `dim: None`

### Open questions

- Should every expression node carry a cached `Ty`, or should type be derivable from a typed environment plus explicit annotations?
- Should symbolic type variables unify during elaboration, or only during later verification?
- Should user-defined functions carry typed signatures in `Context`?
- Should the ML subsystem remain intentionally untyped, or should it be upgraded to typed token streams?

### Recommended decisions

- Use a separate typed IR rather than trying to mutate the current parser AST into a strict type system
- Treat dimensionless as `Concrete(Dimension::dimensionless())`, never as "missing"
- Treat untyped free variables as `Symbol(...)`, never as `None`
- Make every lossy typed-to-untyped conversion explicit in the API and documentation

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
- undeclared variables elaborate to symbolic types, not missing metadata
- typed rewrites preserve type state
- claim-derived rules do not erase units or inline dimensions
- tokenization either preserves type state or uses an explicitly lossy projection
- REPL displays consistent types for `4.5`, `4.5 [m]`, declared vars, and consts with units

Manual checks:
- In the REPL, `4.5` reports `[1]`
- In the REPL, `4.5 [m]` reports `[L]`
- A bare variable with no declaration is shown as having a symbolic unresolved type, not as silently dimensionless
- Ill-typed equalities are rejected before symbolic or numerical equivalence fallback
