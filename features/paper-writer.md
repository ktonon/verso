# TDD for Physics Papers

## Goal

Build a system where physics papers are written in a source format (`.ogma`) with embedded
mathematical claims that are machine-verified. Like TDD: write a claim, see it fail (red),
derive until it passes (green). The source compiles to LaTeX for publication.

## Plan

Six milestones:

| M | Feature | Status |
|---|---------|--------|
| 1 | Red/green loop: `ogma check file.ogma` verifies claims | **completed** |
| 2 | Proof chains + LaTeX compilation | **completed** |
| 3 | Numerical spot-checks (random-point evaluation) | **completed** |
| 4 | Dimensional analysis (`!var` declarations, unit checking) | **completed** |
| 5 | Watch mode (`ogma watch` re-verifies on save) | **completed** |
| 6 | VSCode integration (inline diagnostics via LSP) | **completed** |

### Source format (`.ogma`)

```
# Section Heading

Prose with inline math`sin(x)` and claim references claim`name`.
Raw LaTeX via tex`\vec{v}`.

!claim name
  lhs = rhs

!proof name
  expr0
  = expr1   ; rule_name
  = expr2   ; rule_name

!var velocity [L T^-1]
```

### Key design decisions

- **Verification via `simplify(lhs - rhs) == 0`** rather than normalizing both sides independently. Subtraction + cancellation is more robust.
- **Expression syntax reuses ogma_symbolic's parser**, not LaTeX. Documents compile *to* LaTeX via `ToTex`.
- **Dimensions as annotations**, not in the expression AST. Keeps ogma_symbolic clean for ML pipeline.
- **New crate `ogma_doc`** for document-level concerns (parsing, verification, compilation). Depends on `ogma_symbolic`.

## Implementation Notes

### M1 (completed)

- Created `ogma_doc` crate with: `ast.rs`, `parse.rs`, `verify.rs`, `report.rs`
- CLI binary `ogma_check` accepts `.ogma` files, reports pass/fail per claim with colored output
- Line-oriented parser: `#` headings, `!claim name` blocks with indented `lhs = rhs` body, prose
- Integration tests with fixtures: `basic_algebra.ogma`, `trig_identities.ogma`, `should_fail.ogma`
- npm scripts: `npm run check -- file.ogma`, `npm test` (full workspace tests + lint)

### M2 (completed)

- **Proof chains**: `!proof name` blocks with `= expr ; justification` steps. Each adjacent pair verified via `simplify(from - to) == 0`. Named rules tried first via `RuleSet::find_rule`.
- **Tagged inline expressions**: `math`expr`` (parsed + ToTex), `tex`raw`` (passthrough), `claim`name`` (eqref).
- **LaTeX compiler**: `compile_tex.rs` generates full `\documentclass{article}` with `amsmath`, sections, equations with `\label`, proofs as `align*`, inline math, and `\eqref` for claim references.
- **CLI binary**: `ogma_compile` outputs LaTeX to stdout or `-o file.tex`.
- New fixtures: `proof_chain.ogma`, `full_document.ogma`
- npm script: `npm run compile -- file.ogma`

### M3 (completed)

- **Numerical evaluator**: `eval.rs` with `eval_f64` supporting all Expr variants and `free_vars` extraction.
- **Spot-check fallback**: When symbolic `simplify(lhs - rhs)` doesn't reach 0, `spot_check` evaluates both sides at 200 deterministic pseudo-random points and checks relative error < 1e-8.
- **Three-tier outcomes**: Pass (green ✓, symbolic), NumericalPass (yellow ~, with sample count), Fail (red ✗, with residual).
- **Test fixture**: `numerical_fallback.ogma` with `sinh(x) = (exp(x) - exp(-x))/2` and `cosh(x) = (exp(x) + exp(-x))/2` — true identities the symbolic engine can't prove (no hyperbolic↔exponential rules).

### M4 (completed)

- **Dimension types**: `dim.rs` with `BaseDim` enum (L, M, T, Θ, I, N, J), `Dimension` type (BTreeMap of exponents), parsing from bracket notation `[M L T^-2]`.
- **Dimension inference**: `check_dim(expr, env)` propagates dimensions through all Expr variants — multiplication adds exponents, division subtracts, power multiplies, functions require dimensionless args.
- **Claim checking**: `check_claim_dim(lhs, rhs, env)` with four outcomes: Pass, Skipped (undeclared vars), LhsRhsMismatch, ExprError (e.g., adding length to time).
- **Integration into verifier**: `VerificationResult.dim_outcome` is `Some` when document has `!var` blocks, `None` otherwise. A claim must pass both symbolic and dimensional checks.
- **Parser**: `!var varname [dim spec]` blocks parsed as `Block::Var(VarDecl)`.
- **Report**: dim errors shown in red, skipped in gray.
- New fixtures: `dimensional.ogma` (3 passing claims with dims), `dim_fail.ogma` (dimension mismatch).

### M5 (completed)

- **Watch binary**: `ogma_watch` uses `notify` + `notify-debouncer-mini` for cross-platform file watching with 300ms debounce.
- **Workflow**: Clears terminal and re-verifies all files on each save. Green "Watching..." when all pass, red when failures exist.
- npm script: `npm run watch -- file.ogma`

### M6 (completed)

- **LSP server**: `ogma_lsp` binary implements Language Server Protocol via `tower-lsp`. Publishes diagnostics on open/change/save: errors for failed claims/proof steps/dim mismatches, warnings for numerical-only passes.
- **VSCode extension**: Minimal `editors/vscode/` extension registers `.ogma` language, launches `ogma_lsp`, and receives inline diagnostics.
- **Setup**: `cargo build --release -p ogma_doc --bin ogma_lsp`, then open the extension in VSCode (F5 to debug, or package with `vsce`).

## Verification

```bash
cargo test --package ogma_doc        # unit + integration tests
npm run check -- file.ogma           # verify a document
npm run compile -- file.ogma         # compile to LaTeX
npm run watch -- file.ogma           # watch mode
npm test                            # full workspace tests + lint
```
