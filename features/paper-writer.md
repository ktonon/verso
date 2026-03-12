# TDD for Physics Papers

## Goal

Build a system where physics papers are written in a source format (`.erd`) with embedded
mathematical claims that are machine-verified. Like TDD: write a claim, see it fail (red),
derive until it passes (green). The source compiles to LaTeX for publication.

## Plan

Six milestones:

| M | Feature | Status |
|---|---------|--------|
| 1 | Red/green loop: `erd check file.erd` verifies claims | **completed** |
| 2 | Proof chains + LaTeX compilation | planned |
| 3 | Numerical spot-checks (random-point evaluation) | planned |
| 4 | Dimensional analysis (`:dim` declarations, unit checking) | planned |
| 5 | Watch mode (`erd watch` re-verifies on save) | planned |
| 6 | VSCode integration (inline diagnostics via LSP) | planned |

### Source format (`.erd`)

```
# Section Heading

Prose paragraphs become LaTeX body text.

:claim name
  lhs = rhs

:proof name
  expr0
  = expr1   ; rule_name
  = expr2   ; rule_name

:dim velocity [L T^-1]
```

### Key design decisions

- **Verification via `simplify(lhs - rhs) == 0`** rather than normalizing both sides independently. Subtraction + cancellation is more robust.
- **Expression syntax reuses erd_symbolic's parser**, not LaTeX. Documents compile *to* LaTeX via `ToTex`.
- **Dimensions as annotations**, not in the expression AST. Keeps erd_symbolic clean for ML pipeline.
- **New crate `erd_doc`** for document-level concerns (parsing, verification, compilation). Depends on `erd_symbolic`.

## Implementation Notes

### M1 (completed)

- Created `erd_doc` crate with: `ast.rs`, `parse.rs`, `verify.rs`, `report.rs`
- CLI binary `erd_check` accepts `.erd` files, reports pass/fail per claim with colored output
- Line-oriented parser: `#` headings, `:claim name` blocks with indented `lhs = rhs` body, prose
- Integration tests with fixtures: `basic_algebra.erd`, `trig_identities.erd`, `should_fail.erd`
- npm scripts: `npm run check -- file.erd`, `npm test` (full workspace tests + lint)

## Verification

```bash
cargo test --package erd_doc        # unit + integration tests
npm run check -- file.erd           # verify a document
npm test                            # full workspace tests + lint
```
