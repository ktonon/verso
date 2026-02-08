# ERD Project

A symbolic mathematics library in Rust with expression parsing, simplification, and tensor algebra support.

## Project Structure

```
erd/
├── erd_symbolic/     # Core symbolic math library (Rust)
│   ├── src/
│   │   ├── expr.rs      # Expression AST (Const, Named, Var, Add, Mul, Neg, Inv, Pow, Fn)
│   │   ├── rule.rs      # Pattern matching and rewriting rules
│   │   ├── search.rs    # Beam search simplification algorithm
│   │   ├── parser.rs    # Expression parser (unicode, implicit mul, tensors)
│   │   ├── fmt.rs       # Display formatting with unicode output
│   │   ├── to_tex.rs    # LaTeX output
│   │   └── bin/repl.rs  # Interactive REPL
│   └── Cargo.toml
├── erd_training/     # ML training pipeline (Rust + Burn)
│   ├── src/
│   │   ├── model.rs     # Transformer encoder-decoder (Burn Module)
│   │   ├── train.rs     # Supervised training loop
│   │   ├── rl_train.rs  # REINFORCE training loop
│   │   ├── evaluate.rs  # Model evaluation with direct validation
│   │   ├── vocab.rs     # Encoder/Decoder vocabularies
│   │   ├── dataset.rs   # JSONL data loading + Burn Batcher
│   │   ├── config.rs    # CLI configs (TrainConfig, RLConfig, EvalConfig)
│   │   └── schedule.rs  # Cosine LR schedule with warmup
│   └── Cargo.toml
├── erd_model/        # Model definitions
├── erd_viewer/       # Viewer component
└── erd_app/          # Application layer
```

## Key Design Principles

### Rules Are the Source of Truth

**All expression transformations must be defined as rules.** The search algorithm's job is only to explore the space of transformations made possible by the rules. Do not add special-case logic to `search.rs` for specific transformations—add rules instead.

This separation is critical because:
1. **Rules are declarative** - they describe *what* transformations are valid, not *how* to find them
2. **Search is the strategy** - it decides *which* rules to apply and in what order
3. **ML-ready architecture** - we can eventually replace beam search with a learned model that outputs a sequence of rules to apply

### Prefer Composable Rules Over Specific Ones
Instead of adding specific rules like `(x+1)(y+1) = xy + x + y + 1`, compose from simpler rules:
- Distributive law: `x * (y + z) = xy + xz`
- `x * x = x^2`
- `x^a * x = x^(a+1)`

The beam search explores multiple rewrite paths to find simplifications.

### RuleSets for Organization
Group related rules into RuleSets:
- `RuleSet::standard()` - basic algebraic identities (x+0=x, x*1=x, etc.)
- `RuleSet::trigonometric()` - trig identities (sin²+cos²=1, etc.)
- `RuleSet::tensor()` - tensor algebra (distribution, power rules)
- `RuleSet::factoring()` - factoring patterns (common factor, perfect squares)
- `RuleSet::full()` - combines all of the above

### Named Constants and Exact Types
- `Expr::Rational(Rational)` — exact integers and fractions (e.g. `3`, `1/2`)
- `Expr::FracPi(Rational)` — exact multiples of π (e.g. `π/4` is `FracPi(Rational(1,4))`)
- `Expr::Const(f64)` — user-entered decimals
- Remaining `NamedConst` variants: `E`, `Sqrt2`, `Sqrt3`, `Frac1Sqrt2` (√2/2), `FracSqrt3By2` (√3/2)
- Display as unicode: `π / 2`, `√2 / 2`
- LaTeX: `\frac{\pi}{2}`, `\frac{\sqrt{2}}{2}`

### Pattern Matching
- `Pattern::Const(n)` matches both `Expr::Const(n)` and `Expr::Named(nc)` by value
- Wildcards bind to any expression
- `ConstWild` binds only to constants/named constants

## Important Functions in search.rs

The search module orchestrates rule application—it should not contain transformation logic itself.

- **`simplify()`** - Main entry point; runs beam search with rules, then post-processing
- **`BeamSearch`** - Explores rule application paths, keeps best candidates by complexity
- **`fold_constants()`** - Evaluates constant expressions, detects pi-fractions (legitimate post-processing)
- **`collect_linear_terms()`** - Combines like terms using canonical keys (legitimate post-processing)
- **`canonical_key()`** - Normalizes expressions for comparison (handles Mul commutativity, dummy index normalization)

## REPL Usage

### ML REPL (with beam search fallback)

```bash
npm run repl
```

Commands:
- `:trace` - Toggle step-by-step simplification trace
- `:ml` - ML only mode (no beam search fallback)
- `:beam` - Beam search only mode
- `:hybrid` - ML with beam search fallback (default)
- `:history` - Toggle between input/result history
- `:q` - Quit

### Beam Search REPL (no ML)

```bash
npm run repl:beam
```

### Example inputs
- `pi / 2` → `π / 2`
- `sin(pi / 4)` → `√2 / 2`
- `(x + y + 1)(x + y + 1) - x**2 - y**2 - 1 - 2*x*y - 2*x - 2*y` → `0`

## Testing

```bash
cargo test --package erd_symbolic
```

## Common Patterns

### Adding a New Named Constant
1. Add variant to `NamedConst` enum in `expr.rs`
2. Add `value()` and `from_value()` cases
3. Add Display in `fmt.rs`
4. Add LaTeX in `to_tex.rs`
5. Optionally add folding logic in `try_fold_pi_fraction()` in `search.rs`

### Adding a New Simplification Rule
1. **Always add rules to `rule.rs`**, never add special-case logic to `search.rs`
2. Choose the appropriate RuleSet: `standard()`, `trigonometric()`, `tensor()`, or `factoring()`
3. Use `p_named()` for named constant outputs
4. Prefer multiple simple rules over one complex rule
5. Add test in `search.rs` to verify the beam search finds the simplification

### Adding a New `FnKind` Variant
1. Add variant to `FnKind` enum in `expr.rs`
2. Add Display in `fmt.rs`
3. Add LaTeX in `to_tex.rs`
4. Add parsing in `parser.rs` (`parse_function_call`, `is_known_function`)
5. **Silent bug risk**: Add to `ALL_FN_KINDS` array in `training_data.rs` (no compiler error if missing)
6. **Silent bug risk**: Add to `fn_kind_string` in `training_data.rs`
7. **Silent bug risk**: Add to `parse_token_string` in `training_data.rs` (decides `Token::Fn` vs `Token::FnN`)
8. **Silent bug risk**: Add to `FN_POOL` in `gen_expr.rs` if it should appear in generated training data

### Adding a New `NamedConst` Variant
1. Add variant to `NamedConst` enum in `expr.rs`
2. Add `value()` and `from_value()` cases in `expr.rs`
3. Add Display in `fmt.rs`
4. Add LaTeX in `to_tex.rs`
5. **Silent bug risk**: Add to `ALL_NAMED_CONSTS` array in `training_data.rs`
6. **Silent bug risk**: Add to `named_const_string` in `training_data.rs`
7. **Silent bug risk**: Add to `parse_token_string` in `training_data.rs`

### Adding a New `Expr` Variant
This is the most invasive change. The Rust compiler will catch most missing match arms, but not all.

**Compiler-enforced** (exhaustive match):
1. `expr.rs` — `complexity()`, `Clone`, `PartialEq`, etc.
2. `fmt.rs` — `Display` impl and `Colored` impl
3. `to_tex.rs` — `ToTex` impl
4. `rule.rs` — `Pattern` enum, `match_expr_inner`, `substitute`
5. `search.rs` — `canonical_key`, `eval_constants`
6. `token.rs` — `tokenize`, `detokenize`, `assign_paths`, `subexpr_at`, `replace_subexpr`
7. `random_search.rs` — `all_rewrites_depth`

**Silent bug risk** (arrays/functions that won't trigger compiler errors):
8. `gen_expr.rs` — random expression generation (if the variant should appear in training data)
9. `training_data.rs` — `token_to_string`, `parse_token_string` (for ML token serialization)

### ML Training Pipeline
The training pipeline is pure Rust using the Burn framework. Key commands:

```bash
npm run build:data  # regenerate training data (randomized beam search traces)
npm run train       # supervised training (saves checkpoints/best.mpk)
npm run evaluate    # evaluate model on validation set
npm run rl-train    # REINFORCE fine-tuning (saves checkpoints/rl_best.mpk)
```

RL training auto-resumes from `rl_best.mpk` if it exists. Use `--device ndarray` for CPU (recommended for long RL runs on laptops).

### Invalidating ML Artifacts
Any change to `Expr` variants, `FnKind`, `NamedConst`, rule definitions, or tokenization logic invalidates all training data and checkpoints. To rebuild:

```bash
npm run ml:reset    # removes data_training/ and checkpoints/
npm run build:data  # regenerate training data
npm run train       # retrain from scratch
npm run evaluate    # validate the new model
npm run rl-train    # REINFORCE fine-tuning
```
