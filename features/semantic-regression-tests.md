# Semantic Regression Test Roots

## Goal

Make verso useful as a semantic regression harness for papers by supporting dedicated test documents that validate symbolic declarations, defs, claims, proofs, and dimensional consistency without treating those documents as publishable papers.

A motivating use case is ERD-style work, where a `paper.test.verso` root would exercise the symbolic layer of a paper independently of the main narrative document.

This feature is not about proving the physics of a paper correct. It is about catching regressions in the paper's symbolic layer: renamed symbols, broken defs, dimension mismatches, and proof failures.

## Plan

### Phase 1: Symbol-only import (`use`) — COMPLETE

Implemented in `resolve_includes()` in parse.rs. `use path.verso` reads the target file through the same resolution pipeline as `!include` (circular detection, recursive resolution), then `extract_declarations()` filters to only var/def/func lines and their indented body (descriptions). The rest of the pipeline (verify, compile, LSP) is unchanged — they see the inlined declarations as if they were written directly.

Key files: `verso_doc/src/parse.rs` (resolve_file, extract_declarations), `editors/vscode/syntaxes/verso.tmLanguage.json` (directive-use).

```
use src/notation.verso
use src/dynamics.verso

claim scaling_consistent
  ℓ_{n-1} * σ = ℓ_{n}
```

### Phase 2: Test roots in config

Add a `.verso.jsonc` config field so `verso check` picks up test roots but `verso build` skips them:

```jsonc
{
  "papers": [
    { "input": "src/paper.verso", "output": "paper" }
  ],
  "tests": [
    { "input": "src/paper.test.verso" }
  ]
}
```

Keep it minimal — a test root is just a `.verso` file that is checked but not compiled to PDF.

### Phase 3: `expect_fail`

Add an `expect_fail` block that succeeds only when the enclosed claim or check fails. Essential for testing dimensional mismatches and intentional constraint violations.

```
expect_fail wrong_dimension
  var v [L T^-1]
  var a [L T^-2]
  claim bad
    v = a
```

Note: a `test` keyword was considered but doesn't add value over `claim` — in a test root, claims already don't emit to a PDF. The distinction is test root vs paper root, not test vs claim.

### Phase 4: Machine-readable output (defer)

Add `verso check --json` for CI and git hooks. Defer until CI integration is actually needed — the human-readable output is sufficient for initial use.

### Phase 5: Selective execution (defer)

Add selective test execution (specific test root or changed-only tests). Defer until the test suite is large enough to warrant it.

## Implementation Notes

Verso already provides enough symbolic machinery to make a narrow semantic regression harness useful: defs can be expanded, dimensions can be checked, and proof chains can be validated. The missing piece is ergonomics.

As-is, users can approximate this by adding another paper root such as `paper.test.verso`, but the fit is awkward because papers and tests share the same config and build model.

Key implementation considerations:
- `use` needs to parse the target file, extract `Block::Var`, `Block::Def`, and `Block::Func` declarations, and inject them into the current document's symbol table without emitting any content.
- Test roots should share the same verification engine as paper roots — no separate code path for checking.
- `expect_fail` inverts the verification result for its enclosed block. A passing check inside `expect_fail` is a test failure.

## Verification

- A `use` statement imports symbols from another file and makes them available for claims and proofs.
- A project can declare a test root in `.verso.jsonc` that is checked but not built as a publishable paper.
- At least one `expect_fail` example works and reports success only when the intended failure occurs.

## Deferred Verification

- `verso check --json` emits stable machine-readable results (Phase 4, deferred).
