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

### Named Constants
Mathematical constants (π, e, √2, etc.) are first-class citizens for clean output:
- `NamedConst::Pi`, `FracPi2`, `FracPi3`, `FracPi4`, `FracPi6`
- `NamedConst::Sqrt2`, `Sqrt3`, `Frac1Sqrt2` (√2/2), `FracSqrt3By2` (√3/2)
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

```bash
cargo run --bin repl
```

Commands:
- `:steps` - Toggle step-by-step simplification trace
- `:history` - Toggle between input/result history
- `:q` - Quit

Example inputs:
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
