# Symbol References

## Goal

Make `sym` useful inside flowing prose by turning it into a compact declaration reference instead of an inline glossary expansion. Authors should be able to mention a declared variable or definition inline and let the surrounding sentence stay readable while still pointing readers back to the numbered declaration.

## Plan

- Give `var` and `def` declarations stable equation labels in the TeX compiler.
- Change `sym` rendering so `sym`name`` becomes the symbol plus an equation reference, and `sym`name|display`` becomes the custom display plus the same equation reference.
- Keep `func` support lightweight for now: if no numbered declaration exists, `sym` falls back to a compact inline rendering without inlining description text.
- Update syntax-guide prose and feature docs so the new behavior is documented.

## Implementation Notes

- Added `declaration_equation_label()` in `verso_doc/src/tex_queries.rs` to generate stable ASCII-safe labels for numbered declaration equations.
- `collect_symbols()` now stores an optional `reference_label` on each symbol so prose rendering can reuse the same target that the compiler emits.
- `write_var()` and `write_def()` now emit `\\label{...}` on their numbered equations.
- `ProseFragment::Sym` in `verso_doc/src/tex_prose.rs` now renders compact references instead of appending declaration detail and description text.
- Override text in `sym` continues to go through prose parsing, so emphasis and other inline formatting still work before the reference.

## Verification

- `cargo test -p verso_doc compile_sym_`
- `npm test`
