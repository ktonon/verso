# VSCode Extension Improvements

## Goal

Transform the Verso VSCode extension from a minimal LSP client into a polished
editing experience with syntax highlighting, correct language configuration,
code folding, and snippets.

## Current State (Audit)

The extension currently provides:
- **LSP client** connecting to `verso_lsp` for diagnostics (parse errors, claim verification, dimensional analysis)
- **File association** for `.verso` files
- **Minimal language config** with bracket/paren auto-closing

What's missing or broken:
- **No syntax highlighting** — `.verso` files render as plain text
- **Wrong comment syntax** — configured as `//` but Verso uses `%`
- **No code folding** — blocks, environments, lists can't be collapsed
- **Missing bracket pairs** — no `{ }` matching
- **No snippets** — no templates for common structures
- **No TextMate grammar** — the most impactful gap

## Milestones

| M | Feature | Status |
|---|---------|--------|
| 1 | Fix language configuration (comments, brackets, folding) | **completed** |
| 2 | TextMate grammar: structure and directives | **completed** |
| 3 | TextMate grammar: inline constructs and expressions | **completed** |
| 4 | Snippets for common Verso constructs | **completed** |

## Plan

### Phase 1: Fix language configuration (M1)

Fix incorrect settings and add missing features to `language-configuration.json`.

**Key files:**
- `editors/vscode/language-configuration.json`

**Changes:**
- Comment syntax: `//` → `%` (line comment)
- Add `{ }` to brackets and auto-closing pairs
- Add `"` `"` and `'` `'` auto-closing pairs
- Add folding markers for block-level constructs:
  - Start: `:claim`, `:proof`, `:theorem`, `:lemma`, `:definition`, `:corollary`, `:remark`, `:example`, ` ```math `
  - End: blank line or closing ` ``` `
- Add `surroundingPairs` for brackets, parens, braces, backticks, quotes
- Add `wordPattern` for Verso identifiers

**Tests:**
- Manual: open `.verso` file, verify `Cmd+/` inserts `% ` not `// `
- Manual: verify `{` auto-closes to `}`
- Manual: verify folding arrows appear on claims, proofs, environments

**Estimated scope:** ~30 lines.

---

### Phase 2: TextMate grammar — structure and directives (M2)

Create a TextMate grammar for block-level syntax highlighting.

**Key files:**
- `editors/vscode/syntaxes/verso.tmLanguage.json` (new)
- `editors/vscode/package.json` — register grammar under `contributes.grammars`

**Scopes to define:**

| ERD construct | TextMate scope |
|--------------|----------------|
| `# Heading` | `markup.heading.N.verso` (N=1-4) |
| `% comment` | `comment.line.percentage.verso` |
| `:claim name` | `keyword.control.directive.verso` + `entity.name.tag.verso` |
| `:proof name` | `keyword.control.directive.verso` + `entity.name.tag.verso` |
| `:var name [dims]` | `keyword.control.directive.var.verso` + `variable.parameter.var.verso` + `storage.type.dimension.verso` |
| `:bibliography path` | `keyword.control.directive.verso` + `string.unquoted.verso` |
| `:theorem Title` | `keyword.control.directive.verso` + `entity.name.section.verso` |
| (same for `:lemma`, `:definition`, `:corollary`, `:remark`, `:example`) | same pattern |
| ` ```math ` | `punctuation.definition.fenced.verso` |
| `> block quote` | `markup.quote.verso` |
| `- list item` | `markup.list.unnumbered.verso` |
| `1. list item` | `markup.list.numbered.verso` |
| `= step` (in proofs) | `keyword.operator.proof-step.verso` |
| `; justification` | `comment.line.justification.verso` |

**Design decisions:**
- Use standard TextMate scope naming conventions so existing color themes work out of the box
- Headings use `markup.heading` (like markdown) for theme compatibility
- Directives use `keyword.control` for consistent coloring across themes
- Grammar is a single JSON file using `patterns` and `repository` for organization

**Tests:**
- Manual: open an `.verso` file and verify headings, directives, and comments are colored
- Verify grammar loads without errors in Developer Tools console

**Estimated scope:** ~200 lines.

---

### Phase 3: TextMate grammar — inline constructs and expressions (M3)

Extend the grammar with inline highlighting within prose and math contexts.

**Key files:**
- `editors/vscode/syntaxes/verso.tmLanguage.json`

**Scopes to add:**

| ERD construct | TextMate scope |
|--------------|----------------|
| `` math`...` `` | `markup.inline.math.verso` (tag: `support.function.tag.verso`, content: `meta.embedded.math.verso`) |
| `` tex`...` `` | `markup.inline.tex.verso` |
| `` claim`...` `` | `markup.inline.claim-ref.verso` |
| `` cite`...` `` | `markup.inline.citation.verso` |
| `**bold**` | `markup.bold.verso` |
| `*italic*` | `markup.italic.verso` |
| `^[footnote]` | `markup.other.footnote.verso` |
| Claim body expressions | operators, numbers, functions, variables |

**Expression sub-grammar (inside claims, proofs, math blocks):**
- Numbers: `constant.numeric.verso`
- Operators (`+`, `-`, `*`, `/`, `**`, `^`, `=`): `keyword.operator.verso`
- Known functions (`sin`, `cos`, `sqrt`, etc.): `support.function.math.verso`
- Constants (`pi`, `e`): `constant.language.verso`
- Variables: `variable.other.verso`
- Parentheses/brackets: `punctuation.verso`

**Design decisions:**
- Expression highlighting is shared between claims, proofs, and math blocks via a `#math-expression` repository rule
- Tagged backtick constructs use `begin`/`end` patterns to scope the tag and content separately
- Bold/italic use `markup.bold`/`markup.italic` for maximum theme compatibility

**Tests:**
- Manual: verify inline math tags are colored differently from prose
- Manual: verify bold/italic text appears bold/italic (theme-dependent)
- Manual: verify expression operators and functions are highlighted in claims/proofs

**Estimated scope:** ~150 lines (additions to existing grammar).

---

### Phase 4: Snippets (M4)

Add code snippets for common Verso constructs.

**Key files:**
- `editors/vscode/snippets/verso.json` (new)
- `editors/vscode/package.json` — register under `contributes.snippets`

**Snippets:**

| Prefix | Description | Body |
|--------|-------------|------|
| `claim` | New claim block | `:claim ${1:name}\n  ${2:lhs} = ${3:rhs}` |
| `proof` | New proof block | `:proof ${1:name}\n  ${2:expr}\n  = ${3:step}` |
| `var` | Variable declaration | `:var ${1:name} [${2:dims}]` |
| `thm` | Theorem environment | `:theorem ${1:Title}\n  ${2:body}` |
| `lem` | Lemma environment | `:lemma ${1:Title}\n  ${2:body}` |
| `def` | Definition environment | `:definition ${1:Title}\n  ${2:body}` |
| `mathb` | Math block | `\`\`\`math\n${1:expr}\n\`\`\`` |
| `bib` | Bibliography | `:bibliography ${1:refs.bib}` |

**Tests:**
- Manual: type `claim` + Tab, verify snippet expands with cursor at name
- Manual: verify tab stops progress through placeholders

**Estimated scope:** ~60 lines.

---

### Phase ordering rationale

Phase 1 is a quick fix for incorrect behavior (wrong comment key). Phase 2
delivers the highest-impact improvement — visible syntax highlighting for
document structure. Phase 3 completes highlighting for inline content. Phase 4
adds convenience features. Each phase is independently shippable.

## Verification

After each phase:
1. Run `npm run vscode:install` to rebuild and reinstall
2. Open an `.verso` file (e.g. `verso_doc/tests/fixtures/dimensional.verso`)
3. Verify the feature works as described in the phase tests
4. Check Developer Tools console for grammar/extension errors
