# Conceptual Dimensions

## Goal

Allow papers to introduce non-physical "conceptual" dimensions that participate in the existing dimension-checking machinery. A user can declare, e.g., `Population` or `Currency` once and then write `var n_humans [Population]` and `var n_animals [Population]` so the type checker treats them as compatible quantities.

The motivation is descriptive papers where terms have semantic meaning but no SI dimension. Today the only options are dimensionless (`[1]`, which makes everything compatible) or pretending the term has an SI dimension. Neither expresses intent.

## Plan

### Declaration site: `.ogma.jsonc`

Add a `dimensions` field that lists user-defined base dimension names:

```jsonc
{
  "input": "paper.ogma",
  "outputDirectory": "build",
  "dimensions": ["Population", "Currency", "Probability"]
}
```

Rationale: file-local declarations would force every doc that uses `[Population]` to redeclare it; project-scoped keeps a single source of truth.

### AST: extend `BaseDim`

Add a `User(SmolStr)` (or `String`) variant to `BaseDim` in `ogma_symbolic/src/dim.rs`. Keep the existing SI variants — they are special-cased in physics literature and benefit from being statically known.

```rust
pub enum BaseDim {
    L, M, T, Theta, I, N, J,
    User(SmolStr),
}
```

Update:
- `BaseDim::from_str` — return `User(...)` for unknown names *only when* the name matches a registered conceptual dimension. Bare unknown names should still error so typos don't silently typecheck.
- `Display` — print the user name verbatim.
- `Ord` / `PartialOrd` derive should still work; `User` variants sort after the SI ones.

### Parser: thread the registry

`Dimension::parse` currently takes `&str`. Add `Dimension::parse_with(s, &registry)` where the registry is `&HashSet<SmolStr>` of registered conceptual names. The bare `parse` keeps current behavior (SI only) for tests and REPL use.

Threading the registry from `.ogma.jsonc` → document compilation → wherever `Dimension::parse` is called today is the bulk of the work. Touch points:
- `ogma_doc::parse` — when parsing `var X [...]`, `def X [...] := ...`, `func X [...]` etc.
- `ogma_doc::compile_tex` — when rendering dimensions to LaTeX (probably reads through cleanly via `Display`).
- `ogma_doc::verify` — dimension checking already operates on parsed `Dimension` values, so nothing to change downstream.

### Schema

Update `ogma_doc/schema/v0.1.0/ogma.schema.json` to document the new `dimensions` field as `string[]`.

### LaTeX rendering

User dimension names render as their bare string (no italics, no special font). Let the user override via `tex` blocks if they want a fancier presentation later.

### Errors

- Unknown dimension name with no matching declaration → existing error (`unknown base dimension 'X'`) but extend the message to suggest declaring it in `.ogma.jsonc`.
- Duplicate declaration in config → reject at config parse time.
- Empty / non-identifier names in config → reject at config parse time (must match `[A-Za-z][A-Za-z0-9_]*`).

### Open questions

1. **Casing convention** — should we require user dims to start with uppercase to distinguish from SI base dims that are single-letter (or `Theta`)? My lean is yes (matches `Theta`'s precedent).
2. **REPL** — should the REPL also support conceptual dims? If yes, how are they declared (a `:dim Name` command? read from a config file?). Defer until needed; the REPL today is mostly for symbolic algebra exploration where conceptual types don't add value.
3. **Dimensionless coercion** — should arithmetic between a conceptual dim and a literal number error or pass? Today `[L]` + `2` errors. Conceptual dims should behave the same way (consistency).

## Implementation Notes

_To be filled in during implementation._

## Verification

- Unit tests in `dim.rs` for `BaseDim::User` parsing, display, and dimension algebra (mul/inv/pow/nth_root with mixed SI + user dims).
- Integration test in `ogma_doc` that parses a `.ogma.jsonc` with `dimensions`, parses a paper using one of those dims, and confirms type checking succeeds for compatible uses and fails for incompatible ones.
- Manual check: build a small paper that declares `Population` and uses it for two `var` declarations; confirm the rendered PDF shows the dimension symbol verbatim and that `ogma check` passes.
