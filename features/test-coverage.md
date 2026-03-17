# Test Coverage Improvement

## Goal

Improve test coverage across `verso_symbolic` and `verso_doc` library crates, prioritized by correctness impact and ROI.

## Baseline (2026-03-15)

Overall: **77.9% line coverage** (16,071 lines, 3,549 missed)

## Progress

| Date | Line Coverage | Notes |
|------|--------------|-------|
| 2026-03-15 | 77.9% → 81.7% | All 4 phases complete |

## Plan

### Phase 1 — Highest ROI, pure functions ✓

| File | Baseline | After | Key Gaps |
|------|----------|-------|----------|
| eval.rs | 67% | 83% | ~~`sign()`, `Custom` fn, `Quantity` eval, `Inv(0)`, `FnN` arity~~ |
| fmt.rs | 64% | 96% | ~~`fmt_colored` entirely untested~~ |
| dim.rs | 92% | 94% | ~~Parse error paths, edge cases~~ |

### Phase 2 — Core correctness ✓

| File | Baseline | After | Key Gaps |
|------|----------|-------|----------|
| validate.rs | 62% | 93% | ~~`validate_with_trace` entirely untested~~ |
| context.rs | 67% | 87% | ~~`check_equal` branches, `expand_funcs`, `DimError` display~~ |

### Phase 3 — Remaining gaps ✓

| File | Baseline | After | Key Gaps |
|------|----------|-------|----------|
| to_tex.rs | 85% | 93% | ~~Negative FracPi, `Quantity`, `Custom` fn~~ |
| expr.rs | 86% | 99% | ~~`collect_units` nested paths, `first_unit` variants~~ |
| unit.rs | 83% | 93% | ~~Compound `base_si_display`, unicode prefix, derived unit lookups~~ |

### Phase 4 — Document parser ✓

| File | Baseline | After | Key Gaps |
|------|----------|-------|----------|
| parse.rs | 87% | 88% | ~~Table parsing, URL fragments, error paths, `!include`~~ |
| config.rs | 87% | 93% | ~~File I/O paths, `ConfigError` display~~ |

## Implementation Notes

- Binary entry points (repl.rs, verso.rs, gen_data.rs) are excluded — 0% is expected
- Run `npm run coverage:summary` to check progress
- Run `npm run coverage` for HTML report at `target/llvm-cov/html/index.html`

## Verification

```bash
npm run coverage:summary
```
