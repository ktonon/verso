# Variable, Constant, and Function Declarations

## Goal

Add three declaration directives to verso — `:var`, `:const`, `:func` — using physicist-friendly language. These declarations build up a mathematical context so that claims and proofs can reference named constants, invoke defined functions, and carry dimensional annotations. The repl should support the same directives as a stateful interactive environment.

All core logic lives in `verso_symbolic`. Both `verso_doc` and the repl are thin consumers.

## Background

Currently verso has:
- `:dim` — annotates a variable with physical dimensions (dimensional analysis only)
- `:claim` — asserts `lhs = rhs` and verifies symbolically
- `:proof` — step-by-step chain verifying a claim

Missing capabilities:
- No way to bind a name to a fixed value (physical constants like c, G, h)
- No way to define reusable parameterized expressions (functions like KE, PE)
- `:dim` is programmer jargon; `:var` is more natural for physicists
- The repl only simplifies expressions and checks equalities — no persistent context

Existing duplication to eliminate:
- `is_zero` is defined in both `verso_doc/verify.rs` and `verso_symbolic/repl.rs`
- The equality check pattern (diff, simplify, check zero) is repeated in both
- `DimEnv` lives in `verso_doc` but dimensional analysis is a symbolic concern

## Plan

### Architecture

`verso_symbolic` owns a `Context` struct — the single source of truth for mathematical state. It holds declarations, rules, dimension environment, and proven claims. It provides all core operations: substitution, expansion, equality checking, simplification, and dimensional analysis.

```
verso_symbolic::Context
├── vars: HashMap<String, Option<Dimension>>   // :var declarations
├── consts: HashMap<String, Expr>              // :const bindings
├── funcs: HashMap<String, FuncDef>            // :func definitions
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

**verso_doc** becomes a thin layer:
- Parses directives into declaration types defined in `verso_symbolic`
- Feeds them into a `Context` as it walks the document top-to-bottom
- Calls `context.verify_claim()` — no equality logic of its own

**The repl** becomes a thin layer:
- Parses user input line-by-line
- Feeds declarations into a long-lived `Context`
- Calls `context.simplify()` or `context.check_equal()` — no equality logic of its own

### Directive syntax

**`:var` — declare a variable with optional dimensions**

Replaces `:dim`. Declares a free (universally quantified) variable.

```verso
:var v [L T^-1]
:var θ
:var m [M]
```

Dimensions are optional (dimensionless quantities like angles or counts).

**`:const` — bind a name to a fixed value**

Introduces a named constant. The value is substituted wherever the name appears in subsequent claims and proofs.

```verso
:const c = 3*10^8 [m/s]
:const G = 6.674*10^-11 [m^3 kg^-1 s^-2]
:const pi_approx = 355/113
```

Constants carry dimensions implicitly from their value expression.

**`:func` — define a named parameterized expression**

Introduces a named function that expands at use sites.

```verso
:func KE(m, v) = (1/2) * m * v^2
:func PE(m, h) = m * g * h
:func gamma(v) = 1 / sqrt(1 - v^2 / c^2)
```

Parameters are positional. The body can reference previously declared `:const` and `:var` names.

### Claims as rules

A verified `:claim` becomes a rewrite rule in the `Context` for subsequent claims and proofs. This makes the system compositional.

```verso
:claim pythagorean
  sin(x)^2 + cos(x)^2 = 1

:proof double_angle_cos
  cos(2*x)
  = cos(x)^2 - sin(x)^2
  = cos(x)^2 - (1 - cos(x)^2)  ; pythagorean
  = 2*cos(x)^2 - 1
```

### Repl support

The repl becomes a stateful verso environment:

```
> :var v [L T^-1]
> :const c = 3*10^8 [m/s]
> :func KE(m, v) = (1/2) * m * v^2
> KE(2, 3)
9
> :const m_e = 9.109*10^-31 [kg]
> KE(m_e, 0.1 * c)
4.09905*10^-16 [kg m^2 s^-2]
> sin(x)^2 + cos(x)^2 = 1
true
> :reset
```

The repl is just a readline loop that feeds lines into a `Context`.

### Design decisions

- **`:const` dimensional consistency** is verified at declaration time.
- **Repl equality**: a line containing `=` is treated as a claim (check lhs = rhs). A bare expression is simplified (like an auto-prover).
- **Redefinition** is an error in both documents and the repl. Use `:reset` in the repl to clear state.
- **No piecewise `:func`** for now. Simple expression body only.
- **`:dim` is removed**, not deprecated. Direct rename to `:var`.

### Migration

- Rename `:dim` to `:var` across all existing `.verso` files and tests
- Remove `:dim` parsing entirely (no deprecated alias)
- Update the TextMate grammar for VS Code highlighting
- Move `DimEnv` from `verso_doc` to `verso_symbolic`
- Move `is_zero`, `check_equal`, `verify_claim` logic from `verso_doc/verify.rs` into `verso_symbolic::Context`
- `verso_doc/verify.rs` becomes a thin wrapper that walks the AST and calls `Context` methods

### Key files

**verso_symbolic (owns all logic):**
- `verso_symbolic/src/context.rs` — new: `Context` struct with all state and operations
- `verso_symbolic/src/expr.rs` — add substitution and function expansion methods
- `verso_symbolic/src/repl.rs` — simplify to thin consumer of `Context`

**verso_doc (thin parsing/walking layer):**
- `verso_doc/src/parse.rs` — parse `:var`, `:const`, `:func` into types from `verso_symbolic`
- `verso_doc/src/ast.rs` — reference declaration types from `verso_symbolic`
- `verso_doc/src/verify.rs` — replace with thin wrapper over `Context`

**Editor support:**
- `editors/vscode/syntaxes/verso.tmLanguage.json` — highlight new directives

## Implementation Notes

(To be updated as work progresses.)

## Verification

```bash
cargo test --workspace   # all existing tests still pass
verso check              # existing papers still verify
```

New test cases needed:
- `:var` with and without dimensions
- `:const` substitution in claims
- `:func` expansion in claims and proofs
- Claims used as rules in subsequent proofs
- Repl session with accumulated context
- `:dim` no longer parses (clean removal)
- `verso_doc` verification produces identical results using `Context`
