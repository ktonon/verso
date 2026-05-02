# Symbolic Differentiation

## Goal

Add a `diff(expr, var)` operator to `ogma_symbolic` that returns the symbolic derivative of `expr` with respect to `var`, with rules covering power/sum/product/quotient/chain plus the existing trig/exp/ln functions. End state: `diff(M/r, r)` evaluates to `-M/r^2` and can appear inside `claim` blocks.

The motivating use case is the [tmm paper](../../tmm), which has several places where a symbolic differentiation step is load-bearing (e.g. the math`M/r → -M/r^2` gravitational gradient claim in §5/§7). Without symbolic derivatives the only verifiable claims in that paper are trivial restatements of definitional equalities.

## Plan

### Scope

In:

- Single-variable derivatives: `diff(expr, var)`.
- Algebraic rules: power, sum, product, quotient, constant, identity (`diff(x, x) = 1`, `diff(c, x) = 0` for any `c` not depending on `x`).
- Chain rule for the existing `FnKind` variants (Sin, Cos, Tan, Asin, Acos, Atan, Sinh, Cosh, Tanh, Exp, Ln). Discontinuous functions (Sign, Floor, Ceil, Round, Min, Max, Clamp) return derivative-undefined-here, modelled as a no-op (the `Diff` node remains unevaluated, signalling the caller that the symbolic form couldn't be reduced).
- User-defined functions: expand first via the existing `Context::expand_funcs`, then differentiate. Constants from `def` substitution likewise expanded first.
- Quantities with units: differentiate the inner expression, rebuild with appropriate dimension. (Length / time → velocity dimensions, etc. Out of scope to chase the dimensional algebra fully — keep it dimensionless if it gets ambiguous.)
- Higher-order derivatives compositionally: `diff(diff(f, x), x)` works because the outer `diff` simplifies first, then the inner.

Out:

- Partial derivatives across multiple variables in the same call.
- Implicit differentiation, integration, limits.
- Vector calculus, tensor derivatives, exterior calculus.
- Special notation like `d/dx`. Stick with the function-style `diff(f, x)`.
- Custom user-supplied derivative rules.

### Architecture

1. **`FnKind::Diff` variant** in `ogma_symbolic/src/expr.rs`. `Diff` is a 2-argument function and uses the existing `FnN(FnKind, Vec<Expr>)` machinery — no new `ExprKind` variant required.
2. **New `derivative.rs` module** in `ogma_symbolic/src/`. Public entry point `differentiate(expr: &Expr, var: &str) -> Expr`. Recursively walks the expression and rewrites according to the rules.
3. **`eval_derivatives` pass** in `search.rs`, mirroring `eval_constants`. When the simplifier encounters a `FnN(Diff, [expr, var])` node, it calls `differentiate` and replaces the node. Runs alongside `eval_constants` in the main `simplify` pipeline.
4. **Parser**: `diff(f, x)` syntax. `FnN` parsing already exists (`clamp(x, lo, hi)`); register `diff` as a known multi-arg function name in `parser.rs`.
5. **Display/LaTeX**: render as `diff(f, x)` in unicode, math`\frac{d}{dx}\left(f\right)` in LaTeX. The LaTeX form requires extracting the variable name from the second argument; if the second argument isn't a simple `Var`, fall back to the unicode form.
6. **`training_data.rs` updates** for tokeniser awareness — silent-bug-risk per `CLAUDE.md`.

### Differentiation rules

In `derivative.rs::differentiate(expr, var)`:

- **Constants** (Rational, FracPi, Named, Quantity-with-no-var, Var-but-name-doesn't-match-target): return `Rational(0)`.
- **Var matching target**: return `Rational(1)`.
- **Add(a, b)**: `differentiate(a) + differentiate(b)`.
- **Mul(a, b)**: `differentiate(a) * b + a * differentiate(b)` (product rule).
- **Neg(a)**: `-differentiate(a)`.
- **Inv(a)**: `-differentiate(a) * a^(-2)` (derivative of 1/a, special case of power rule).
- **Pow(base, exp)**:
  - If exp doesn't depend on var: `exp * base^(exp-1) * differentiate(base)` (power rule + chain).
  - If base doesn't depend on var: `base^exp * ln(base) * differentiate(exp)` (exponential rule + chain).
  - General case: `base^exp * (differentiate(exp) * ln(base) + exp * differentiate(base) / base)` (logarithmic differentiation).
- **Fn(kind, arg)** for the trig/exp/ln cases: standard chain-rule applications.
  - `diff(sin(u), x) = cos(u) * diff(u, x)`
  - `diff(cos(u), x) = -sin(u) * diff(u, x)`
  - `diff(exp(u), x) = exp(u) * diff(u, x)`
  - `diff(ln(u), x) = (1/u) * diff(u, x)`
  - etc.
- **Fn(kind, arg)** for discontinuous cases (Sign, Floor, etc.): leave the `Diff` node unevaluated.
- **FnN(Custom(name), args)**: expand via context-aware substitution if a `func` definition is available; otherwise leave unevaluated.
- **FnN(Diff, [inner_expr, inner_var])**: recursively differentiate the inner Diff first (already a Diff node — it should have been simplified before reaching here, but if not, evaluate it and continue).
- **Quantity(inner, unit)**: differentiate inner; preserve unit if unambiguous, drop if not.

After applying the rule, run a simplification pass on the result so common-subexpression collapse and zero-elimination happen.

### Parser changes

In `parser.rs`, register `diff` as a 2-argument known function alongside `clamp`. The variable argument should accept any expression syntactically (the differentiation logic only uses it if it's a Var; otherwise it's a no-op).

### Display / LaTeX changes

In `fmt.rs::Display`:

```
FnKind::Diff => write!(f, "diff({}, {})", arg1, arg2)
```

In `to_tex.rs::ToTex`:

```rust
FnKind::Diff => {
    if let ExprKind::Var { name, .. } = &args[1].kind {
        write!(f, "\\frac{{d}}{{d{}}}\\left({}\\right)", name, args[0].to_tex())
    } else {
        write!(f, "\\mathrm{{diff}}\\left({}, {}\\right)", args[0].to_tex(), args[1].to_tex())
    }
}
```

### Tests

In `ogma_symbolic/tests/derivatives.rs` (or as inline tests in `search.rs`):

- Power rule: `diff(x^3, x) = 3*x^2`
- Power rule with constant: `diff(x^n, x) = n*x^(n-1)` (n declared as another var, treated as constant).
- Sum: `diff(x + x^2, x) = 1 + 2*x`
- Product: `diff(x * sin(x), x) = sin(x) + x*cos(x)`
- Quotient: `diff(M/r, r) = -M/r^2` (the tmm-motivating case).
- Chain: `diff(sin(x^2), x) = 2*x*cos(x^2)`
- Constant: `diff(5, x) = 0`
- Identity: `diff(x, x) = 1`
- Different var: `diff(y, x) = 0`
- Combined: `diff(exp(-x^2/2), x) = -x*exp(-x^2/2)`

Plus a fixture in `ogma_doc/tests/fixtures/derivatives.ogma` that exercises `diff(...)` inside `claim` blocks end-to-end.

## Implementation Notes

2026-05-01: Implemented and tested. Final layout:

- `FnKind::Diff` added to `expr.rs`. Two-argument variant; uses `FnN(Diff, vec![expr, var])`.
- `pub fn diff(expr, var)` constructor in `expr.rs`.
- New module `ogma_symbolic/src/derivative.rs` with `pub fn differentiate(expr, var) -> Expr`. Recursively walks the expression and applies standard rules. Discontinuous functions (Sign, Floor, Ceil, Round) and unhandled FnN cases (Min, Max, Clamp, Custom, nested Diff) are left as unevaluated `Diff` nodes.
- `pub fn eval_derivatives(expr)` added to `search.rs`. Walks bottom-up and resolves any `FnN(Diff, [expr, var])` node where the second argument is a Var.
- `eval_derivatives` wired into the main `simplify` pipeline at the very start, before beam search and constant folding.
- Parser: added `"diff" => diff(args.remove(0), args.remove(0))` arm in `parse_function_call`. Two-argument like `clamp`.
- Display: `FnKind::Diff => "diff"`.
- LaTeX: special-case in the `FnN` arm that renders `Diff` as `\frac{d}{dx}\left( ... \right)` when the variable argument is a `Var`, falls back to `\operatorname{diff}\left( ..., ... \right)` otherwise.
- `training_data.rs`: added `FnKind::Diff` to `fn_kind_string` (returns `"DIFF"`), to `ALL_FN_KINDS`, and to `parse_token_string`. Updated `build_vocab_metadata_sanity` test count from 75 to 76.
- Tests: 11 unit tests in `derivative.rs::tests` covering constant, identity, power, sum, product, quotient (the tmm motivating case), chain, exp, ln, constant-times-var. Plus an end-to-end fixture `ogma_doc/tests/fixtures/derivatives.ogma` with 9 claims that all pass via `ogma check`.

One implementation detail worth noting: when expressing `d/dx (1/u) = -u' / u^2` in `differentiate_inv`, the original implementation used `pow(u, -2)` which produces `u^-2` — a form the simplifier treats as different from the `1/u^2` form the parser produces from `M/r^2`. The fix was to use `inv(pow(u, 2))` instead, which matches the parser's canonical form. Generic algebraic-equivalence canonicalisation in the simplifier could remove this constraint in the future, but the explicit-form approach is sufficient for now.

The earlier failure mode where the LSP showed "passed numerically but not symbolically" warnings was caused by a stale ogma binary in the IDE — the freshly built binary verifies all claims symbolically as expected.

## Verification

A successful implementation will:

- Pass `cargo test --package ogma_symbolic --package ogma_doc --release`.
- Run the new derivatives fixture via `ogma check ogma_doc/tests/fixtures/derivatives.ogma` with all claims passing.
- The tmm paper can add a `claim phase_gradient_falloff: diff(M/r, r) = -M/r^2` block in `src/gravity.ogma` or `src/particles-and-interactions.ogma` and have it verify under `ogma check`.
