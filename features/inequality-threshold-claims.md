# Inequality and Threshold Claims

## Goal

Add first-class support for inequality-style claims in Verso so papers can state and check threshold conditions such as confinement onset, stability bounds, and admissibility regions.

The immediate motivating case is ERD, where the next kernel step wants statements like `Λ > 1` and `Π_{in} >= Π_{el}`. These are not full PDE claims; they are constitutive or regime-defining inequalities that should be expressible and checkable in the same document workflow as equality claims.

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

Expected failure kinds should distinguish at least:

- `comparison_false`
- `dimension_mismatch`
- `dimension_error`

### Phase 3: Verification strategy

Verification should proceed in this order:

1. Dimension check both sides exactly as for equality claims
2. If both sides reduce to comparable constants, decide the comparison symbolically
3. Otherwise fall back to numerical sampling, similar to current equality verification

For numerical fallback, the check should succeed only if all samples satisfy the requested inequality. Output should still indicate when the result was numerical rather than symbolic.

### Phase 4: Minimal domain constraints

Inequalities become much more useful once variables can be constrained to a domain. The initial domain feature should stay narrow:

- positive-only variables
- nonnegative variables
- optional closed intervals for scalar quantities

Possible surfaces:

```verso
var x [1]
  domain: x > 0
```

or

```verso
assume positive_x
  x > 0
```

The exact syntax can be decided separately, but inequality verification will need some notion of admissible sample region.

## Implementation Notes

The current `erd-support.md` note identifies inequalities as a need but does not specify a concrete surface, AST shape, or verification order. This feature narrows that work to the first implementation slice needed for threshold statements.

This should not be blocked on derivatives, vector calculus, or integrals. A useful first release can handle scalar comparisons only.

Design constraints:

- keep comparison claims close to existing `claim` semantics
- reuse current dimension checking before any numerical fallback
- make failure reasons stable enough for `expect_fail ... [comparison_false]`
- avoid over-designing domain syntax in the first pass

## Verification

- A paper can write inequality claims with `>`, `>=`, `<`, and `<=`
- Dimension mismatches in inequality claims fail with the expected reason
- At least one constant inequality is verified symbolically
- At least one variable inequality is verified numerically
- `expect_fail` can target a false inequality with a stable reason such as `comparison_false`
