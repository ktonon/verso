# Bang Directives

## Goal

Change the directive/command prefix from `:` to `!` across ogma documents and the REPL. This frees `:` for unicode completions (`:mu:` → μ), which uses an established convention from GitHub/Slack/Discord.

## Motivation

The `:` character previously served as the prefix for document directives and REPL commands. Switching to `!` frees `:` for unicode completions (`:name:` trigger) and is visually cleaner:

- `!` is visually distinct and easy to spot in prose
- Rare at the start of a sentence in natural language
- Single character, easy to type
- Markdown precedent: `![]()` for images

**Tradeoff**: `!` cannot be used for logical negation in the REPL if binary expressions are added later. Alternatives like `not` keyword or `¬` (via `:not:` completion) would work.

## Scope

### 27 unique directives/commands to rename

| Category | Items |
|----------|-------|
| Document metadata | `title`, `author`, `date`, `abstract` |
| Layout | `toc`, `pagebreak`, `center` |
| Content blocks | `figure`, `table`, `claim`, `proof`, `bibliography` |
| Declarations | `var`, `const`, `func` |
| Environments | `theorem`, `lemma`, `definition`, `corollary`, `remark`, `example` |
| Include | `include` |
| REPL control | `q`/`quit`/`exit`, `reset`, `trace` |
| REPL history | `history`/`hist` |

### Files to modify

| File | Occurrences | Notes |
|------|-------------|-------|
| `ogma_doc/src/parse.rs` | ~145 | Directive parsing, error messages, tests |
| `ogma_symbolic/src/repl.rs` | ~21 | REPL command parsing |
| `editors/vscode/language-configuration.json` | 1 | Folding markers regex |
| `editors/vscode/snippets/ogma.json` | 9 | Snippet prefixes and bodies |
| `ogma_doc/tests/fixtures/*.ogma` | ~105 lines | 11 test fixture files |
| `editors/vscode/tests/*.ogma` | TBD | VSCode test fixtures |
| Feature files, CLAUDE.md, README | Implicit | Documentation references |

## Plan

1. **Rust sources**: Replace `:` prefix with `!` in `parse.rs` and `repl.rs` — primarily string literals and `starts_with` checks
2. **Test fixtures**: Update all `.ogma` files
3. **VSCode extension**: Update snippets, folding markers, syntax grammar
4. **Documentation**: Update CLAUDE.md, README, feature files
5. **Run full test suite** to confirm nothing breaks

The rename is mechanical (find-replace) but touches many files. Best done in a single commit.

## Implementation Notes

Mechanical find-replace across the entire codebase. Key changes:

- `ogma_doc/src/parse.rs`: `starts_with('!')` for directive detection (lines 620, 633, 862). Line 322 `starts_with(':')` preserved — it's table alignment detection, not a directive.
- `ogma_symbolic/src/repl.rs`: All REPL commands (`!q`, `!trace`, `!var`, etc.)
- `ogma_doc/src/ast.rs`, `verify.rs`, `compile_tex.rs`: Doc comments and test strings
- `ogma_symbolic/src/context.rs`, `expr.rs`: Doc comments
- `ogma_doc/src/bin/ogma.rs`, `ogma_training/src/bin/ml_repl.rs`: CLI help text
- All `.ogma` test fixtures (11 files in `ogma_doc/tests/fixtures/`, 2 in `editors/vscode/tests/`)
- VSCode extension: `tmLanguage.json` regex patterns, `snippets/ogma.json` prefixes/bodies, `language-configuration.json` folding markers
- `CLAUDE.md` and all feature files updated

## Verification

- `cargo test --release` — full workspace passes
- `ogma check` on all test fixtures succeeds
- VSCode snippets and folding still work
- REPL commands respond to `!` prefix
