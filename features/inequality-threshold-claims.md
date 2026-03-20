# Inequality and Threshold Claims

## Goal

Add first-class support for inequality-style claims in Verso so papers can state and check threshold conditions such as confinement onset, stability bounds, and admissibility regions.

The immediate motivating case is ERD, where the next kernel step wants statements like `Λ > 1` and `Π_{in} >= Π_{el}`. These are not full PDE claims; they are constitutive or regime-defining inequalities that should be expressible and checkable in the same document workflow as equality claims.

This first slice should be conservative. Verso should reject nonsense, preserve the comparison operator in diagnostics, and prove comparisons only when it can do so honestly. It should not overstate what has been established.

## Plan

### Phase 1: Comparison syntax in claims

Extend `claim` so its body can contain one binary comparison operator:

- `>`
- `>=`
- `<`
- `<=`

Initial scope should exclude chained comparisons such as `0 < x < 1`.

Example:

```verso
claim confinement_onset
  Λ > 1

claim persistence_floor
  Π_{in} >= Π_{el}
```

### Phase 2: AST and reporting support

Represent comparison claims explicitly in the AST rather than encoding them as special equality cases. Diagnostics and check output should preserve the operator used in the source so failures are easy to interpret.

Expected outcomes should distinguish at least:

- `comparison_false`
- `dimension_mismatch`
- `dimension_error`
- `comparison_unknown`

### Phase 3: Conservative verification strategy

Verification should proceed in this order:

1. Dimension check both sides exactly as for equality claims
2. If both sides reduce to comparable constants, decide the comparison exactly
3. Otherwise report the comparison as unknown rather than guessing

This v1 should explicitly avoid numerical sampling and variable-domain syntax. Those can be added later once Verso has a principled assumption model.

## Implementation Notes

The current `erd-support.md` note identifies inequalities as a need but does not specify a concrete surface, AST shape, or verification order. This feature narrows that work to the first implementation slice needed for threshold statements.

This should not be blocked on derivatives, vector calculus, or integrals. A useful first release can handle scalar comparisons only.

Design constraints:

- keep comparison claims close to existing `claim` semantics
- reuse current dimension checking before any truth evaluation
- make failure reasons stable enough for `expect_fail ... [comparison_false]`
- add an explicit non-success path for valid-but-undecidable comparisons
- avoid numerical fallback until the project has a principled assumption/domain model
- avoid bundling domain syntax into this feature; treat it as a separate later feature

ERD is the motivating driver, but this feature should generalize cleanly to other scientific papers. The goal is not to make Verso say “true” for more inequalities; the goal is to let authors write comparison claims precisely while keeping the verifier epistemically honest.

## Verification

- A paper can write inequality claims with `>`, `>=`, `<`, and `<=`
- Dimension mismatches in inequality claims fail with the expected reason
- At least one constant inequality is verified exactly
- A dimensionally valid but undecidable symbolic inequality reports `comparison_unknown`
- `expect_fail` can target a false inequality with a stable reason such as `comparison_false`
- No numerical sampling is used in this first implementation slice
