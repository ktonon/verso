# Span-Based Error Reporting

## Goal

Add source location spans to expressions so that dimensional analysis errors (and other errors) can point to the exact part of the input that failed. For example:

```
> 3 * 10 ^ (8 [m/s])
              ^^^^^
dim error: dimension mismatch in exponent: expected [1], got [L T^-1]
```

Instead of a generic error message, the user sees which subexpression caused the failure, making debugging expressions far more intuitive.

## Plan

Break into four phases:

### Phase 1: Span type and Expr integration

Add a `Span { start: usize, end: usize }` type and attach spans to `Expr` nodes (either by wrapping each node or adding a span field to every variant). This is the heaviest phase since it touches every `Expr` construction site.

### Phase 2: Parser span recording

Thread span recording through the parser. Every `parse_*` method records start/end positions from the token stream so that each produced `Expr` carries an accurate span.

### Phase 3: Carry spans through errors

Extend `DimError` variants (and any other error types) to carry the span of the subexpression that failed. This lets error-reporting code know exactly which part of the source is responsible.

### Phase 4: Formatted error output

Format errors using the original source text and the span. Render an underline (caret row) pointing at the offending span, as shown in the goal example.

## Implementation Notes

### Phase 1 (commit 29441a3)
Refactored `Expr` from an enum to a struct wrapping `ExprKind`, with a `Span { start, end }` field. `PartialEq` compares only `kind` (ignores span). Touched 16 files for the Expr→ExprKind migration.

### Phase 2 (commit c0fc1bc)
Tokenizer now returns `Vec<(Token, Span)>` with character offset tracking. Every `parse_*` method records `start_pos()` at entry and sets `expr.span = Span::new(start, self.prev_end)` on the result. Added span tests covering atoms, addition, quantity, nested power+unit, function calls, and dimension annotations.

### Phase 3 (commit 7191284)
All `DimError` variants now carry a `Span`: `UndeclaredVar(String, Span)`, `Mismatch { ..., span }`, `NonDimensionlessFnArg { ..., span }`, `NonIntegerPower(Span)`. The span comes from the subexpression that caused the error (e.g., `expr.span` for the "got" side of a mismatch).

### Phase 4
Added `DimError::span()` accessor and `format_dim_error(error, source, prefix_width)` function in `context.rs`. This renders a caret underline row aligned to the error span, followed by the error message, both in red. The REPL's three error display sites (`:const`, equality, expression) now call `format_dim_error` with the correct source string and prefix width to align carets under the prompt line.

### Post-review fixes
- **Span provenance through apply_consts**: `substitute_consts` and `expand_funcs` now use `Expr::spanned(..., expr.span)` to preserve original spans on compound nodes. Substituted const/func bodies inherit the call-site span so error carets point at the user's input, not the definition site.
- **Byte/char offset mixing in REPL**: replaced `input.len()` with `input.chars().count()` in the two REPL offset calculations so that multibyte characters (π, θ, etc.) don't shift carets.

### Scope

This feature covers REPL-only underline support. Document verification (`report.rs`) and LSP diagnostics (`verso.rs`) still report line-level errors without expression-level span underlines. Extending to documents would require threading source text through the verification pipeline — left as future work.

## Verification

Dimensional analysis errors in the REPL should show underlined source locations pointing at the exact subexpression that caused the error. Unit tests assert that error spans cover the correct character ranges, including after const substitution and function expansion.
