# REPL UX

## Goal

Improve the user experience of the verso REPL for mathematicians and physicists. Address confusing behavior, missing numeric evaluation, and parser limitations that make the tool harder to use interactively.

## Plan

### Incorrect / Missing Numeric Evaluation

1. ~~**`0/0` evaluates to `0`**~~ **Fixed.** REPL detects `Inv(0)` before simplification and reports "division by zero is undefined".
2. ~~**`sqrt(4)` not evaluated**~~ **Fixed.** `eval_constants` now evaluates `Pow(rational, 1/2)` when numerator and denominator are perfect squares.
3. ~~**`abs(-3)` not evaluated**~~ **Fixed.** `eval_constants` now evaluates `Custom("abs")` on rational arguments.
4. ~~**`floor(3/2)` and `ceil(3/2)` not evaluated**~~ **Fixed.** `eval_constants` now evaluates `Floor`, `Ceil`, `Round`, and `Sign` on rational arguments.
5. ~~**`(-x)^2` simplifies to `x·x` instead of `x^2`**~~ **Fixed.** `eval_constants` now strips `Neg` from `Pow(Neg(x), even)` and collects `Mul(x, x)` into `Pow(x, 2)`.

### Parser Limitations

6. ~~**Negative exponents require parens**~~ **Won't fix.** `2^-3` fails; must write `2^(-3)`. Parens are a reasonable workaround.
7. ~~**Double negation prefix fails**~~ **Won't fix.** `--x` fails; must write `-(-x)`. Parens are a reasonable workaround.
8. ~~**Cannot multiply/divide unit quantities**~~ **Fixed.** Unit brackets now bind per-operand within `parse_multiplicative` when the first operand has a unit annotation. `eval_constants` combines `Quantity * Quantity` into a single Quantity with merged units.

### Confusing Behavior

9. ~~**`sin(2x)` displays as `sin(x·2)`**~~ **Fixed.** Formatter now normalizes coefficient-first ordering: when the right operand of a scalar `Mul` is a `Rational`, it displays the coefficient before the variable (both in REPL and LaTeX output).
10. **`F = m * a` is `false` after declaring typed variables** — `!var F [M L T^-2]` and `!var m [M]`, `!var a [L T^-2]` makes a user expect `F = m*a` to be true (since dimensions match), but the REPL only checks symbolic equality, not dimensional equivalence. Could be addressed with documentation or a hint in the output.
11. **`3 [km]` immediately converts to `3000 [m]`** — Units are eagerly converted to SI base during simplification. A physicist typing `3 [km]` expects to see the value preserved in their chosen unit.
12. **Dim errors shown alongside equality results** — `a + b = b + a` with mixed-dimension vars shows a dim error warning *and* `true`. The warning is useful but the combination looks contradictory.

## Implementation Notes

- Bug tests added in `repl::tests` as regular assertions (prefixed `bug_`). All 7 bug tests pass.
- Items 6-7 (parser) closed as won't fix — parens are a reasonable workaround.
- Items 9-12 (confusing behavior) are tracked for future improvement.

## Verification

- `cargo test --release -p verso_symbolic -- repl::tests` exercises all e2e session tests including the bug-fix tests.
