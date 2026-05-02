# Multi-arg user-defined functions in claims

## Goal

Fix two bugs that prevented multi-arg user-defined functions from being usable inside `claim` blocks: a parser issue with underscore-containing function names, and a substitution issue where parameter bindings could trigger infinite recursion.

## Background

Calling user-defined funcs from within `claim` blocks revealed two failure modes:

1. **`apparent_speed(x, y, z)` fails with `Expected("RParen")`.** The tokeniser splits `apparent_speed` into `Ident("apparent")`, `Underscore`, `Ident("speed")`. The expression parser sees `apparent`, doesn't see an LParen next (it sees Underscore), falls through to the scalar+indices path, treats `_speed` as a bare subscript, then tries to read `(x, y, z)` as a parenthesised expression and trips on the comma.

2. **`func myf(n, k) := k^n; claim test: myf(n+1, k) = k^(n+1)` causes a stack overflow.** When binding func parameters, `substitute_consts` recurses to handle chained `def`s. With binding `n → n+1`, the substitution looks up `n` inside the value `n+1`, finds the same binding, and loops forever. The identity case `k → k` likewise recurses indefinitely.

## Plan

### Parser fix: glue compound names back

When the parser sees `Ident(base)` and detects a trailing `(_ Ident)+ LParen` pattern, glue the parts together into a compound name and call `parse_function_call`. Limit to base names of length > 1 to preserve the existing single-char-implies-implicit-mul behaviour.

Implementation in `ogma_symbolic/src/parser.rs`:

- New helper `try_compound_function_name(base) -> Option<(name, tokens_consumed)>` that scans forward for `_Ident _Ident ... LParen` and returns the glued name if found.
- In the `Ident` arm of `parse_atom`, after the existing `name.len() > 1 && peek == LParen` check, add a fallback: if `try_compound_function_name` returns `Some`, consume the matched tokens and call `parse_function_call` with the compound name.

### Substitution fix: separate non-recursive substitution path

The recursive substitution behaviour in `substitute_consts` is correct for chained `def` references (e.g. `def c_{t} := c_{s}; def c_{s} := sqrt(...)`). It is *wrong* for func parameter binding, where a parameter binding `n → n+1` shouldn't follow the `n` inside `n+1` back to itself.

Implementation in `ogma_symbolic/src/context.rs`:

- New `substitute_locals(expr, bindings)` — single-pass substitution that does NOT recurse into the bound value. Same shape as `substitute_consts` minus the recursive call.
- `expand_funcs` switched to use `substitute_locals` for both `Fn(Custom, ...)` and `FnN(Custom, ...)` parameter binding.
- `substitute_consts` keeps its recursive behaviour for chained `def`s, but with an added identity-binding guard for safety: if the binding is `name → name`, return the value as-is rather than recursing.

### Tests

Add fixture cases to `ogma_doc/tests/fixtures/` covering:

- Compound-name function calls inside claims (`apparent_speed(x, y, z) = ...`).
- Func parameter binding with non-trivial expressions (`myf(n+1, k) = k^(n+1)`).
- Recursive-pattern claims (`apparent_speed(n+1, k, c) = k * apparent_speed(n, k, c)`).

## Implementation Notes

2026-05-01: Implemented and tested. Both failing cases now pass:

- `apparent_speed(x, y, z)` parses correctly via `try_compound_function_name`.
- `apparent_speed(n+1, k, c) = k * apparent_speed(n, k, c)` verifies symbolically via `substitute_locals` (no infinite recursion on `n → n+1`).

The original cyclic-substitution risk in `substitute_consts` is also guarded by the new `is_identity_binding` check, so even chained-def recursion can't accidentally loop on `name → name`. Other cyclic patterns in chained defs (`def a := b; def b := a`) are not detected — those are user errors and the old behaviour (stack overflow) is unchanged for them.

The full ogma_symbolic and ogma_doc test suites still pass.

## Verification

- 11 derivative tests + 9 derivatives fixture claims still pass.
- All existing tests in ogma_symbolic and ogma_doc pass.
- The tmm paper's `apparent_speed_recursive` claim verifies symbolically.
