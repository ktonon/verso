# Lean Backend Migration (Deferred)

## Status

**Planned — deferred 2026-04-23.**

Researched and scoped but not started. Revisit only if the current symbolic engine proves insufficient for ERD work. Tool-building is not the goal; ERD is.

## Goal

Replace the custom symbolic inference engine (`ogma_symbolic` rule-based rewriter + beam search) with a Lean 4 backend (Mathlib + PhysLean + dimensional-analysis library from arXiv 2509.13142). Keep the existing markup syntax, REPL, VSCode extension, and ML training scaffolding — they become a frontend and translation layer over Lean.

## Why Deferred

- Lean has a steep learning curve (weeks to months to productivity).
- Mathlib imports consume multiple GB of memory and 30–60s cold start.
- Physics papers rarely need kernel-level proof — dimensional consistency, algebraic correctness, and numerical sanity (all handled today) cover ~95% of real errors.
- [Lean4Physics benchmark](https://arxiv.org/html/2510.26094v1) shows formalizing physics in Lean is hard even for top models (Sonnet 35% on college physics), a proxy for human cost too.
- No current ERD claim has been identified that the existing engine fails on and Lean would succeed on. Without that evidence, the pivot is pre-optimization.

## Trigger to Revisit

Resume this feature when **any** of these become true:

1. A specific ERD claim fails in the current Ogma engine and requires Lean-class rigor.
2. The target publication venue or reviewers demand machine-checked proofs.
3. The current engine's coverage gap blocks ERD progress, and a lighter escape hatch (e.g., SymPy bridge) is insufficient.

## Alternatives Considered

- **Stay with current engine** (chosen). Dimensional analysis and symbolic equivalence already work.
- **SymPy bridge** (lightweight fallback if needed later). Python subprocess, small footprint, symbolic CAS for claims the Rust engine can't handle.
- **Full Lean pivot** (this feature — deferred).

## Migration Plan (for reference when revisited)

### Module fate map

**Keep (frontend/IR):**
- `ogma_symbolic/src/parser.rs`, `token.rs`, `unicode.rs` — input UX
- `ogma_symbolic/src/expr.rs`, `rational.rs`, `dim.rs`, `unit.rs` — becomes IR
- `ogma_symbolic/src/fmt.rs`, `to_tex.rs` — rendering
- All of `ogma_doc/` except `verify.rs`, `dim.rs`, `eval.rs`
- `editors/vscode/` — only protocol payloads change
- `ogma_training/src/vocab.rs`, `policy_model.rs`, `policy_dataset.rs`, `schedule.rs` — transformer scaffolding

**Delete (the engine):**
- `ogma_symbolic/src/rule.rs`, `search.rs`, `random_search.rs`, `gen_expr.rs`, `training_data.rs`, `validate.rs`, `eval.rs`
- Simplification half of `ogma_symbolic/src/context.rs`
- `ogma_symbolic/src/bin/gen_data.rs`, `bin/validate.rs`
- `ogma_training/src/ml_simplify.rs`, `evaluate.rs`

**Rewrite:**
- `ogma_doc/src/verify.rs` — routes claims to Lean backend
- `ogma_symbolic/src/repl.rs` — Lean-backed, keeps input/history UX
- `ogma_training/src/policy_train.rs`, `policy_rl_train.rs` — new targets (tactics, not rules)

**New:**
- `ogma_lean/` crate — `emit` (Expr→Lean) + `backend` (LSP/lake driver)
- `lean/` submodule — Lake project pinning Mathlib, PhysLean, dim-analysis lib

### Translation-layer architecture

`Expr` AST becomes the IR. Emitter is a recursive visitor producing Lean terms. Two modes:
- **Plain mode**: `variable (x : ℝ)`; claims become `example : a = b := by ring` / `norm_num` / `field_simp; ring`.
- **Dimensioned mode**: `var x [L T^-1]` → `variable (x : Quantity D)` using dim-analysis library. Unit mismatches caught at type level.

Tactic routing by claim shape: polynomial → `ring`; rational → `norm_num`; trig → `simp [Real.sin_sq_add_cos_sq, …]`; inequalities → `nlinarith` / `positivity`. Unknown → `sorry` surfaced as `ComparisonUnknown`.

Proof steps: each step becomes one `example : e_i = e_{i+1} := by <tactic>` goal.

### Backend strategies

Two backends behind a `LeanBackend` trait:
1. **`lake build`** — batch, for `ogma check` CLI. Seconds latency amortized over a document.
2. **`lake serve` + LSP** — persistent, for REPL and VSCode. First-session warmup 30–60s, warm roundtrip <1s.

Lean project as git submodule at `./lean/` with pinned Mathlib, PhysLean, dim-analysis revs.

### ML pipeline repurposing

Current head predicts `(rule_index, position)`. New target **tactic classifier**: given `(Expr_lhs, Expr_rhs, context)`, predict from ~50 tactic templates (`ring`, `norm_num`, `field_simp; ring`, `simp [lemma]`, `linarith`, `nlinarith`, `positivity`, `decide`, …).

Encoder stays; swap rule vocab for tactic vocab. RL reward becomes `lake build` exit code — existing REINFORCE loop adapts directly.

Dataset bootstrap: run existing doc corpus through Lean with each tactic template, record first success per claim.

### Phases

1. **Hello-world Lean roundtrip.** `ogma_lean` crate, six `ExprKind` variants, one `ring` test end-to-end, old engine still live as fallback.
2. **Coverage parity (dimensionless).** Full `ExprKind` coverage + tactic router. Measure pass-rate vs current engine on `features/` test corpus. Spike outcome determines whether ML (phase 6) becomes load-bearing earlier.
3. **LSP backend + REPL.** Persistent `lake serve`, rewrite REPL, VSCode points at new diagnostics.
4. **Dimensions.** PhysLean/`Quantity` mode. Migrate `expect_fail [dimension_mismatch]`.
5. **Delete old engine.** Remove rule/search/training-data code. Rename `ogma_symbolic` → `ogma_ir`. Repo rename.
6. **ML retarget.** Tactic classifier, then RL with Lean-build reward.

## Open Questions Before Phase 1

1. Lean vendoring: git submodule vs Lake dependency vs separate repo.
2. `sorry` → `ComparisonUnknown` acceptable UX, or hard-fail?
3. Does ML stay in-tree after decoupling from rule sequences, or fork?
4. Multi-step proofs: user supplies Lean tactic names in `justification`, or auto-route?

## Biggest Technical Unknown

**Tactic closure coverage.** Current engine proves a narrow class (polynomial + trig identities + rational arithmetic). `ring`/`norm_num`/`field_simp`/`nlinarith` cover most but not all. Phase 2 spike would measure pass-rate on existing corpus. If <80%, ML-based tactic search becomes a prerequisite rather than a nice-to-have.

## References

- [Formalizing Dimensional Analysis in Lean 4 (arXiv 2509.13142)](https://arxiv.org/html/2509.13142v1)
- [Lean4Physics (arXiv 2510.26094)](https://arxiv.org/html/2510.26094v1)
- [PhysLean](https://github.com/HEPLean/PhysLean) / [physlean.com](https://physlean.com/)
- [leanclient Python LSP client](https://github.com/oOo0oOo/leanclient)
- [Lean Blueprint (Tao on PFR)](https://terrytao.wordpress.com/2023/11/18/formalizing-the-proof-of-pfr-in-lean4-using-blueprint-a-short-tour/)
