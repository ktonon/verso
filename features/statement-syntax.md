# Statement Syntax Overhaul

## Goal

Unify the statement syntax across the REPL and paper writer, treating verso as a language rather than a collection of directives. Drop the `!` prefix from statements (language constructs) while keeping it for REPL-only commands (tool controls). Introduce `:=` as the definition operator, replacing `!const` and `!definition` with `def`. Align vocabulary across both surfaces.

## Plan

### Phase 1: Parser — drop `!` from statements, introduce `:=`

Update the document parser (`verso_doc/src/parse.rs`) and REPL parser (`verso_symbolic/src/repl.rs`) to recognize the new syntax. Both `!`-prefixed and bare forms need to be handled during transition testing, but the old syntax is removed, not deprecated.

**Statements (shared language, no `!` prefix):**

| Old | New | Notes |
|-----|-----|-------|
| `!var v [L T^-1]` | `var v [L T^-1]` | Unchanged except prefix |
| `!const c = 3` | `def c := 3` | Keyword + operator change |
| `!func sq(x) = x^2` | `func sq(x) := x^2` | Keyword stays, operator changes to `:=` |
| `!definition label` | `def label` | Keyword change, `:=` in body |
| `!claim label` | `claim label` | Unchanged except prefix |
| `!proof label` | `proof label` | Unchanged except prefix |

**REPL shorthands (no keyword needed):**

| Input | Meaning |
|-------|---------|
| `F := m*a` | Implicit `def` |
| `a + b = b + a` | Implicit `claim` (equality check) |

**REPL-only commands (keep `!` prefix):**

`!trace`, `!reset`, `!history`, `!q` — these control the tool, not the language.

**Reserved words:** `var`, `def`, `func`, `claim`, `proof`. These are reserved at line-start only (in documents) or as the first token (in REPL). They can still appear in prose mid-paragraph.

Changes needed:
- `verso_doc/src/parse.rs` — update line-start detection for all statement types
- `verso_symbolic/src/repl.rs` — update `Session::eval` to handle bare keywords and `:=`
- `verso_symbolic/src/parser.rs` — add `:=` token support for REPL implicit defs
- All test fixtures and inline test strings

### Phase 2: Definition semantics — merge `!const` and `!definition` into `def`

`def` is a single-line statement with an optional indented description, same pattern as `var`:

```
def c := 3 * 10^8 [m / s^2]
  The speed of light in a vacuum.

def F := m * a
  Newton's second law.
```

This replaces both `!const` (simple bindings) and `!definition` (named definitions). The name on the left of `:=` is the identifier. The value is substituted in subsequent expressions and registered as a rewrite rule. The optional description appears in LSP hover, while `sym` uses the declaration as a compact inline reference.

For paper output, `def` renders as a numbered equation (like old `!definition`). The left-side name serves as the label for cross-references.

Changes needed:
- `verso_doc/src/ast.rs` — merge `ConstDecl` and `Definition` into a unified `DefDecl` variant
- `verso_doc/src/parse.rs` — parse `def name := expr` with optional indented description
- `verso_doc/src/compile_tex.rs` — render `def` as numbered equation
- `verso_doc/src/verify.rs` — handle `def` in verification (dim-checked, not symbolically verified)
- `verso_symbolic/src/repl.rs` — `:=` handling for implicit defs
- `verso_symbolic/src/context.rs` — unified `declare_def` or similar

### Phase 3: func uses `:=`

Update `func` declarations to use `:=` instead of `=`.

Changes needed:
- `verso_doc/src/parse.rs` — expect `:=` in func body
- `verso_symbolic/src/repl.rs` — update func parsing
- Test fixtures

### Phase 4: Update VS Code extension

Update the TextMate grammar, snippets, and LSP integration.

Changes needed:
- `editors/vscode/syntaxes/verso.tmLanguage.json` — update patterns for all statement types (drop `!`, add `:=`)
- `editors/vscode/snippets/verso.json` — update snippets
- `verso_doc/src/bin/verso.rs` — update LSP hover/goto for new syntax
- `verso_doc/src/compile_tex.rs` — update `find_decl_line` for new syntax

### Phase 5: Update syntax guide and docs

Update the syntax guide to reflect the new syntax and serve as the canonical reference.

Changes needed:
- `verso_doc/tests/fixtures/syntax_guide.verso` — rewrite all examples
- `CLAUDE.md` — update project documentation
- REPL help text (`verso_symbolic/src/repl.rs`) — update `?` output
- Any other `.verso` test fixtures

## Implementation Notes

### Phase 1 (completed)
- Dropped `!` prefix from all six statement keywords: `var`, `const`, `func`, `claim`, `definition`, `proof`
- Document metadata directives (`!title`, `!abstract`, `!table`, etc.) keep `!` prefix
- Parser uses `trimmed == "keyword" || trimmed.starts_with("keyword ")` for safe matching
- REPL help now distinguishes "Statements" (no `!`) from "Commands" (`!`)
- Updated all test fixtures (`.verso` files, inline test strings in parse.rs, compile_tex.rs, verify.rs, repl.rs)

### Phase 2 (completed)
- Renamed `ConstDecl` → `DefDecl`, `Block::Const` → `Block::Def` throughout AST
- Parser now recognizes `def name := expr` with `:=` operator (not `=`)
- Removed `definition` keyword — old `!definition` blocks (unverified claims) are now expressed as `def`
- Removed `is_definition` field from `Claim` struct (always false now)
- Removed LaTeX `\begin{definition}` environment rendering (def is invisible like old const)
- REPL supports bare `:=` as implicit def shorthand (e.g. `c := 3*10^8`)
- REPL help updated: `const` → `def`, examples show `:= `syntax
- `collect_symbols` reports kind as `"def"` instead of `"const"`
- `find_decl_line` now matches `def ` prefix (with `:` in split chars for `:=`)
- Updated all fixture files and test strings

### Phase 3 (completed)
- `func name(params) = body` → `func name(params) := body`
- Updated both document parser and REPL parser
- Updated REPL func output format and help text
- Updated all test fixtures

### Phase 4 (completed)
- TextMate grammar: removed `!` from `claim`, `proof`, `var`, `func` patterns
- TextMate grammar: removed `definition` alternative from claim pattern
- TextMate grammar: renamed `directive-const` → `directive-def`, updated regex to `^(def)\s+(\S+)\s*(:=)\s*(.+)$`
- TextMate grammar: updated `func` pattern to use `:=` instead of `=`
- Snippets: updated `claim`, `proof`, `var`, `func` to drop `!` prefix
- Snippets: replaced `Constant` (`!const name = value`) with `Definition` (`def name := value`)
- Snippets: removed old `Definition` snippet (`!definition`)

### Phase 5 (completed)
- Fixed stale `!var`, `!const`, `!func` references in syntax guide prose
- Fixed stale `!` prefixes in doc comments (`ast.rs`, `parse.rs`)
- Removed `!definition` from environment doc comment (no longer a keyword)

## Verification

- `npm test` — all unit tests pass
- `cargo test -p verso_doc` — document parser and compiler tests
- `cargo test -p verso_symbolic -- repl::tests` — REPL session tests
- Syntax guide compiles without errors
- VS Code extension highlights new syntax correctly
- REPL `?` help reflects new commands
