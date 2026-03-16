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

(empty)

## Verification

Dimensional analysis errors in the REPL should show underlined source locations pointing at the exact subexpression that caused the error. Unit tests should assert that error spans cover the correct byte ranges.
