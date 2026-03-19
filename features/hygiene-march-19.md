# Hygiene Review - March 19

## Goal

Capture a repo hygiene review for `verso`, using the syntax guide as the product
surface and the codebase as the implementation, with emphasis on architecture,
duplication, module organization, trait usage, API documentation, and test
coverage.

## Plan

- Review `verso_doc/tests/fixtures/syntax_guide.verso` as the executable language
  spec.
- Inspect the crate boundaries and the main document/symbolic pipelines for
  cohesion, duplication, and extensibility.
- Validate the current test, lint, and coverage posture and record the highest
  leverage follow-up work.

## Implementation Notes

- 2026-03-19: Phase 1 started by documenting the intended crate-root APIs and
  turning `lib.rs` files into curated facades for `verso_doc`, `verso_symbolic`,
  and `verso_training`.
- 2026-03-19: Phase 2 reduced duplication in `verso_doc/src/verify.rs` by
  extracting shared block execution for top-level and nested verification.
- 2026-03-19: Phase 3 started by extracting parser source-loading and include
  resolution logic into `verso_doc/src/source.rs`, reducing the scope of
  `verso_doc/src/parse.rs` without changing its public API.
- 2026-03-19: Phase 3 continued by extracting label/ref/symbol query helpers
  from `verso_doc/src/compile_tex.rs` into `verso_doc/src/tex_queries.rs`,
  leaving the compile module more focused on rendering.
- The syntax guide is a strength. It is both readable product documentation and a
  regression fixture, and `verso_doc/tests/integration.rs` already verifies that it
  parses, verifies, and compiles.
- Highest-priority finding: the document pipeline is strongly coupled around the
  `Block` enum, but the implementation is spread across large independent passes.
  Adding a new language feature means touching the AST, parser, verifier, LaTeX
  compiler, reporting, tests, and often the syntax guide. The coupling is visible in
  `verso_doc/src/ast.rs`, `verso_doc/src/parse.rs`, `verso_doc/src/verify.rs`, and
  `verso_doc/src/compile_tex.rs`. A visitor/pass abstraction or a narrower set of
  feature-oriented modules would reduce the cross-cutting edit cost.
- Second finding: the public API surface is much broader than the documented API
  surface. `verso_doc/src/lib.rs` exports every module directly, `verso_symbolic/src/lib.rs`
  re-exports entire internal subsystems with wildcard-style root exports, and
  `verso_training/src/lib.rs` does the same for training internals. That makes the
  effective public API large and unstable, while crate-level documentation is still
  thin. A curated facade layer plus `#![deny(missing_docs)]` on intended public
  crates would improve API hygiene substantially.
- Third finding: `verso_doc/src/verify.rs` contains duplicated block-processing
  logic in `verify_document` and `verify_blocks`. The two functions repeat the same
  declaration registration and verification flow with only context initialization
  differing. Extracting a shared executor over `Block` slices would reduce
  maintenance risk.
- Fourth finding: the symbolic core has some traversal helpers on `Expr`
  (`walk`, `any`, `find_map`), but not a reusable transform/fold abstraction. As a
  result, multiple modules still open-code recursive `ExprKind` matches for tree
  traversal or rebuilding, notably in `verso_symbolic/src/search.rs`,
  `verso_symbolic/src/token.rs`, `verso_symbolic/src/eval.rs`, and
  `verso_symbolic/src/to_tex.rs`. This is a good place to use traits more
  aggressively, for example with an `ExprVisitor`/`ExprFolder` pattern.
- Fifth finding: test coverage is strong in the core libraries but weak in
  orchestration-heavy entry points. `cargo llvm-cov --workspace --summary-only`
  currently reports 85.15% total line coverage, with strong coverage in
  `verso_doc/src/verify.rs` (89.05%), `verso_symbolic/src/context.rs` (87.13%),
  `verso_symbolic/src/rule.rs` (97.78%), and `verso_symbolic/src/search.rs`
  (91.74%). The biggest gaps are in binary and training orchestration files such as
  `verso_doc/src/bin/verso.rs`, `verso_training/src/policy_train.rs`,
  `verso_training/src/policy_rl_train.rs`, and `verso_training/src/ml_simplify.rs`.
- Suggested follow-up order:
  1. Introduce a shared document-pass abstraction for `Block` processing.
  2. Curate the public crate facades and document them explicitly.
  3. Extract shared verification execution from `verify_document`/`verify_blocks`.
  4. Add an expression transform trait to reduce recursive boilerplate.
  5. Add focused tests around CLI/training orchestration seams.

## Phased Refactoring Plan

### Phase 1 - Stabilize Public Surfaces

- Define an intended public API for each crate and move toward facade-style exports.
- Add crate-level docs describing the supported entry points for `verso_doc`,
  `verso_symbolic`, and `verso_training`.
- Reduce root-level re-exports to the types and functions that are meant to stay
  stable for downstream callers.

Success criteria:

- `lib.rs` files read as curated entry points rather than index files.
- New contributors can tell which APIs are internal versus supported.
- Status: completed on 2026-03-19.

### Phase 2 - Unify Document Passes

- Introduce a shared document traversal/execution layer for `Block` processing.
- Refactor parsing, verification, reporting, and LaTeX compilation to consume a
  more structured pass boundary where practical.
- Extract the duplicated verification flow in `verify_document` and
  `verify_blocks` behind a shared block executor.

Success criteria:

- Adding a new block kind requires fewer cross-file edits.
- Verification logic for top-level and nested blocks lives in one place.
- Status: completed on 2026-03-19.

### Phase 3 - Decompose Oversized Modules

- Split `verso_doc/src/parse.rs` into smaller feature-oriented parsing modules,
  such as block parsing, inline parsing, include resolution, and shared helpers.
- Split `verso_doc/src/compile_tex.rs` into preamble/meta generation, block
  renderers, and prose/inline rendering helpers.
- In `verso_symbolic`, identify similar seams in `context.rs`, `rule.rs`, and
  `search.rs` so future work lands in smaller ownership areas.

Success criteria:

- Core modules become easier to scan and reason about.
- Tests can target smaller units without relying on monolithic files.

### Phase 4 - Add Expression Traversal Abstractions

- Introduce a reusable traversal/folding abstraction for `Expr`, such as
  `ExprVisitor`, `ExprFolder`, or a small transform API.
- Migrate open-coded recursive tree rebuilding in `search`, `token`, `eval`,
  and `to_tex` when it reduces repetition.
- Keep the abstraction lightweight so it improves maintainability without making
  simple transforms harder to read.

Success criteria:

- Fewer repeated `match ExprKind` traversal blocks across modules.
- New symbolic features can reuse shared traversal primitives.

### Phase 5 - Close Testing and Tooling Gaps

- Add focused tests around CLI orchestration and training orchestration seams,
  especially the behavior currently concentrated in binaries and top-level
  training pipelines.
- Preserve the syntax guide fixture as a top-level integration artifact and grow
  it when language features expand.
- Track coverage by module so architectural hot spots stay visible during
  refactors.

Success criteria:

- Coverage improves in the currently low-signal orchestration files.
- Refactors have regression protection at both unit and workflow levels.

### Recommended Order

1. Phase 1, to reduce API drift before reshaping internals.
2. Phase 2, to eliminate the most obvious cross-cutting duplication.
3. Phase 3, once the shared pass boundaries are clearer.
4. Phase 4, after module seams are cleaner and easier to standardize.
5. Phase 5 throughout, with extra emphasis after each structural refactor.

## Verification

```bash
npm test
npm run coverage:summary
```

- `npm test` passed.
- `cargo llvm-cov --workspace --summary-only` reported 85.15% total line coverage.
