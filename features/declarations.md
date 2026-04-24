# Variable, Constant, and Function Declarations

## Goal

Add three declaration directives to ogma — `!var`, `!const`, `!func` — using physicist-friendly language. These declarations build up a mathematical context so that claims and proofs can reference named constants, invoke defined functions, and carry dimensional annotations. The repl should support the same directives as a stateful interactive environment.

All core logic lives in `ogma_symbolic`. Both `ogma_doc` and the repl are thin consumers.

## Background

Currently ogma has:
- `!var` — declares a variable with optional physical dimensions
- `!claim` — asserts `lhs = rhs` and verifies symbolically
- `!proof` — step-by-step chain verifying a claim

Missing capabilities:
- No way to bind a name to a fixed value (physical constants like c, G, h)
- No way to define reusable parameterized expressions (functions like KE, PE)
- The repl only simplifies expressions and checks equalities — no persistent context

Resolved in Phase 1:
- All core logic (is_zero, check_equal, DimEnv, dimensional analysis) now lives in `ogma_symbolic::Context`
- Both `ogma_doc` and the repl are thin consumers

## Plan

### Architecture

`ogma_symbolic` owns a `Context` struct — the single source of truth for mathematical state. It holds declarations, rules, dimension environment, and proven claims. It provides all core operations: substitution, expansion, equality checking, simplification, and dimensional analysis.

```
ogma_symbolic::Context
├── vars: HashMap<String, Option<Dimension>>   // !var declarations
├── consts: HashMap<String, Expr>              // !const bindings
├── funcs: HashMap<String, FuncDef>            // !func definitions
├── rules: RuleSet                             // built-in + proven claims
├── dims: DimEnv                               // dimensional environment
│
├── fn declare_var(&mut self, name, dim)
├── fn declare_const(&mut self, name, expr)
├── fn declare_func(&mut self, name, params, body)
├── fn add_claim_as_rule(&mut self, name, lhs, rhs)
│
├── fn substitute(&self, expr) -> Expr         // expand consts + funcs
├── fn simplify(&self, expr) -> Expr           // substitute then simplify
├── fn check_equal(&self, lhs, rhs) -> EqualityResult
├── fn check_dims(&self, lhs, rhs) -> DimResult
└── fn verify_claim(&self, claim) -> VerificationResult
```

**ogma_doc** becomes a thin layer:
- Parses directives into declaration types defined in `ogma_symbolic`
- Feeds them into a `Context` as it walks the document top-to-bottom
- Calls `context.verify_claim()` — no equality logic of its own

**The repl** becomes a thin layer:
- Parses user input line-by-line
- Feeds declarations into a long-lived `Context`
- Calls `context.simplify()` or `context.check_equal()` — no equality logic of its own

### Directive syntax

**`!var` — declare a variable with optional dimensions**

Declares a free (universally quantified) variable.

```ogma
!var v [L T^-1]
!var θ
!var m [M]
```

Dimensions are optional (dimensionless quantities like angles or counts).

**`!const` — bind a name to a fixed value**

Introduces a named constant. The value is substituted wherever the name appears in subsequent claims and proofs.

```ogma
!const c = 3*10^8 [m/s]
!const G = 6.674*10^-11 [m^3 kg^-1 s^-2]
!const pi_approx = 355/113
```

Constants carry dimensions implicitly from their value expression.

**`!func` — define a named parameterized expression**

Introduces a named function that expands at use sites.

```ogma
!func KE(m, v) = (1/2) * m * v^2
!func PE(m, h) = m * g * h
!func gamma(v) = 1 / sqrt(1 - v^2 / c^2)
```

Parameters are positional. The body can reference previously declared `!const` and `!var` names.

### Claims as rules

A verified `!claim` becomes a rewrite rule in the `Context` for subsequent claims and proofs. This makes the system compositional.

```ogma
!claim pythagorean
  sin(x)^2 + cos(x)^2 = 1

!proof double_angle_cos
  cos(2*x)
  = cos(x)^2 - sin(x)^2
  = cos(x)^2 - (1 - cos(x)^2)  ; pythagorean
  = 2*cos(x)^2 - 1
```

### Repl support

The repl becomes a stateful ogma environment:

```
> !var v [L T^-1]
> !const c = 3*10^8 [m/s]
> !func KE(m, v) = (1/2) * m * v^2
> KE(2, 3)
9
> !const m_e = 9.109*10^-31 [kg]
> KE(m_e, 0.1 * c)
4.09905*10^-16 [kg m^2 s^-2]
> sin(x)^2 + cos(x)^2 = 1
true
> !reset
```

The repl is just a readline loop that feeds lines into a `Context`.

### Design decisions

- **`!const` dimensional consistency** is verified at declaration time.
- **Repl equality**: a line containing `=` is treated as a claim (check lhs = rhs). A bare expression is simplified (like an auto-prover).
- **Redefinition** is an error in both documents and the repl. Use `!reset` in the repl to clear state.
- **No piecewise `!func`** for now. Simple expression body only.
### Migration (completed in Phase 1 & 2)

- `!dim` renamed to `!var` across all `.ogma` files, tests, parser, AST, and editor support
- `DimEnv`, `is_zero`, `check_equal` moved to `ogma_symbolic::Context`
- `ogma_doc/verify.rs` is a thin wrapper that walks the AST and calls `Context` methods
- AST types renamed: `Block::Dim(DimDecl)` → `Block::Var(VarDecl)`

### Key files

**ogma_symbolic (owns all logic):**
- `ogma_symbolic/src/context.rs` — new: `Context` struct with all state and operations
- `ogma_symbolic/src/expr.rs` — add substitution and function expansion methods
- `ogma_symbolic/src/repl.rs` — simplify to thin consumer of `Context`

**ogma_doc (thin parsing/walking layer):**
- `ogma_doc/src/parse.rs` — parse `!var`, `!const`, `!func` into types from `ogma_symbolic`
- `ogma_doc/src/ast.rs` — reference declaration types from `ogma_symbolic`
- `ogma_doc/src/verify.rs` — replace with thin wrapper over `Context`

**Editor support:**
- `editors/vscode/syntaxes/ogma.tmLanguage.json` — highlight new directives

## Implementation Notes

### Phase 1: Context extraction (completed)
- Moved `is_zero`, `check_equal`, `DimEnv`, dimensional analysis to `ogma_symbolic::Context`
- Both `ogma_doc` and repl are thin consumers

### Phase 2: `!dim` → `!var` rename (completed)
- Renamed across all code, tests, fixtures, editor support, and docs
- AST: `Block::Dim(DimDecl)` → `Block::Var(VarDecl)`

### Phase 3: `!const` support (completed)
- Parser: `!const name = expr`
- Context: `declare_const`, `apply_consts` substitutes before simplification
- Tests: const substitution in claims and proofs

### Phase 4: `!func` support (completed)
- Parser: `!func name(params) = expr`
- Context: `declare_func`, `expand_funcs` replaces `FnKind::Custom` calls
- Multi-character names followed by `(` parse as function calls; single-char remain implicit multiplication
- Function bodies can reference constants (substituted after expansion)

### Phase 5: Claims as rules (completed)
- Verified claims become LTR rewrite rules with free vars as wildcards
- `verify_document` processes blocks in order (single pass)
- `add_claim_as_rule` converts Expr to Pattern

### Phase 6: Repl declarations (completed)
- Repl supports `!var`, `!const`, `!func`, `!reset`
- Passed equality checks registered as rules

### Phase 7: VS Code grammar (completed)
- TextMate patterns for `!const` and `!func` directives
- Snippets for all three declaration types

## Verification

```bash
cargo test -p ogma_symbolic -p ogma_doc --release
cd editors/vscode && npm test
```

All test cases implemented:
- `!var` declaration parsing
- `!const` substitution in claims and proofs, wrong value detection
- `!func` expansion (single param, multi param, with constants)
- Claims used as rules in subsequent claims
- Parser tests for error cases (missing `=`, missing params, etc.)
