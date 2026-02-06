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

### Prefer Composable Rules Over Specific Ones
Instead of adding specific rules like `(x+1)(y+1) = xy + x + y + 1`, compose from simpler rules:
- Distributive law: `x * (y + z) = xy + xz`
- `x * x = x^2`
- `x^a * x = x^(a+1)`

The beam search explores multiple rewrite paths to find simplifications.

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

- **`simplify()`** - Main entry point; runs beam search, constant folding, term collection, and expansion
- **`fold_constants()`** - Evaluates constant expressions, detects pi-fractions
- **`collect_linear_terms()`** - Combines like terms using canonical keys
- **`expand_products()`** - Distributes multiplication over addition
- **`canonical_key()`** - Normalizes expressions for comparison (handles Mul commutativity, Mul(x,x) = Pow(x,2))
- **`extract_term()`** - Extracts coefficient from term (handles Neg inside Mul)

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
1. Add rule in `rule.rs` `RuleSet::standard()` or `::trigonometric()`
2. Use `p_named()` for named constant outputs
3. Add test in `search.rs`
