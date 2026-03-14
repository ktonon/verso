# SI Unit System

## Goal

Add SI unit support to erd expressions so that numeric values can carry unit annotations (e.g., `3*10^8 [m/s]`), enabling dimensional verification through units and unit-aware arithmetic with automatic SI prefix selection for display.

## Plan

### Context-sensitive bracket syntax

- `v [L T^-1]` — variable + dimension (existing, unchanged)
- `3*10^8 [m/s]` — numeric expression + unit (new)
- `c [m/s]` — SYNTAX ERROR (variable requires dimension, not unit)
- `3 [L]` — SYNTAX ERROR (number requires unit, not dimension)

### Unit types (`erd_symbolic/src/unit.rs`)

- `BaseUnit` enum: m(L), g(M, scale 0.001), s(T), K(Theta), A(I), mol(N), cd(J)
- `DerivedUnit` table: N, J, W, Pa, Hz, C, V, Ohm
- `SiPrefix` enum: p, n, mu, m, c, k, M, G, T
- `Unit { dimension, scale, display }` with mul/inv/pow
- `lookup_unit()` — tries exact derived, exact base, "kg" special case, prefix+derived, prefix+base
- `best_prefix()` — choose human-readable SI prefix for display

### The kg problem

Kilogram is the SI base but already carries a prefix. Solution: `g` (gram) is the parseable base with scale 0.001. `kg` = k(1000) * g(0.001) = 1.0.

### Expr::Quantity

New variant `Quantity(Box<Expr>, Unit)` wrapping a numeric expression with its unit. Internal arithmetic converts to base SI via `eval(inner) * unit.scale`.

### Dimensional verification

Units imply dimensions: `[m/s]` automatically carries `[L T^-1]`. `infer_dim` for Quantity returns `unit.dimension`.

### Unit-aware arithmetic

All values convert to canonical base SI form internally. Display chooses suitable SI prefix (e.g., 0.001 m -> 1 mm, 3000 m -> 3 km).

## Implementation Notes

### Created files
- `erd_symbolic/src/unit.rs` — core unit types and parsing (10 tests)
- `erd_symbolic/src/dim.rs` — Dimension type moved from erd_doc (shared across crates)

### Modified files
- `erd_symbolic/src/lib.rs` — added `pub mod unit`, `pub mod dim`, re-exports
- `erd_symbolic/src/expr.rs` — `Quantity(Box<Expr>, Unit)` variant, `quantity()` constructor
- `erd_symbolic/src/parser.rs` — context-sensitive bracket parsing with `parse_unit_bracket`, `expr_has_vars` helper, improved error messages for dimension/unit mismatches
- `erd_symbolic/src/fmt.rs` — Display and colored output for Quantity
- `erd_symbolic/src/to_tex.rs` — LaTeX rendering: `{value} \; \mathrm{{unit}}`
- `erd_doc/src/dim.rs` — `infer_dim` for Quantity returns `unit.dimension`; uses erd_symbolic::Dimension
- `erd_doc/src/eval.rs` — `eval_f64` for Quantity: `eval(inner) * unit.scale`
- `erd_symbolic/src/search.rs` — pass-through for Quantity
- `erd_symbolic/src/token.rs` — pass-through for Quantity
- `erd_symbolic/src/gen_expr.rs` — Quantity as leaf
- `erd_symbolic/src/random_search.rs` — Quantity as leaf

### Key design decisions
- Brackets at the multiplicative level: `3*10^8 [m/s]` wraps the entire multiplicative expression
- `expr_has_vars()` determines context: variables → dimension brackets, numeric → unit brackets
- Improved parser errors: "variables require dimensions, not units" / "numeric values require units, not dimensions"
- Dimension/unit symbols disambiguated by context, not by the symbol itself (N = Newton for numbers, N = amount-of-substance dimension for variables)

## Verification

```bash
cd /Users/ktonon/repos/erd && cargo test --release
```

All tests pass (723+). Integration tests verify:
- Parsing: `3 [m]`, `5 [km]`, `3*10^8 [m/s]`, `10 [kg*m/s^2]`, `100 [N]`, `5 [1/s]`
- Error cases: `c [m/s]` (var+unit), `3 [L]` (number+dim)
- Round-trip: `parse → display → reparse` preserves equality
- Dimensional analysis: quantities carry correct dimensions, claim checking works
- Evaluation: quantities convert to base SI (5 km → 5000, 2 kg → 2, 500 mg → 0.0005)
