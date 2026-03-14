# Paper Readiness

## Goal

Make ERD output complete, submission-ready LaTeX without manual post-processing.
Today ~50-70% of a real physics paper can be written in ERD; the rest (metadata,
figures, tables, custom formatting) requires editing the generated LaTeX by hand.
This feature tracks closing those gaps.

## Milestones

| M | Feature | Status |
|---|---------|--------|
| 1 | Document metadata (title, author, date, abstract) | completed |
| 2 | Figures with captions and labels | completed |
| 3 | Tables (markdown-style) | completed |
| 4 | Custom preamble / document class | completed |
| 5 | Multi-file include | completed |
| 6 | Unresolved ref diagnostics | completed |
| 7 | Page breaks | planned |

## Plan

### M1: Document metadata

Add directives for front matter that compile to standard LaTeX title block.

**Syntax:**
```
:title Quantum Corrections to the Classical Limit
:author Alice Smith
:author Bob Jones
:date 2026-03-13
:abstract
  We present a novel approach to computing quantum corrections
  in the semiclassical regime. Our method yields exact results
  for the harmonic oscillator and perturbative results for
  anharmonic potentials.
```

**Key files:**
- `erd_doc/src/ast.rs` — add `Block::Title`, `Block::Author`, `Block::Date`, `Block::Abstract`
- `erd_doc/src/parse.rs` — detect `:title`, `:author`, `:date`, `:abstract` directives
- `erd_doc/src/compile_tex.rs` — emit `\title{}`, `\author{}`, `\date{}`, `\begin{abstract}...\end{abstract}`, `\maketitle`

**Design decisions:**
- Multiple `:author` directives are joined with `\and`
- `:date` is optional; omitted → `\date{}` (no date)
- `:abstract` body is indented continuation lines, parsed for inline fragments
- `\maketitle` emitted after `\begin{document}` when any metadata is present
- Metadata directives can appear anywhere but always compile to the preamble/front matter

**Tests:**
- Parse each directive individually
- Compile document with all metadata → verify `\title`, `\author`, `\date`, `\maketitle`, `\begin{abstract}`
- Multiple authors joined with `\and`
- Abstract with inline math
- Document without metadata → no `\maketitle`

---

### M2: Figures

Add a figure directive for including images with captions and labels.

**Syntax:**
```
:figure plots/energy-levels.pdf
  caption: Energy levels of the hydrogen atom as a function of principal quantum number.
  label: fig-energy-levels
  width: 0.8
```

**Key files:**
- `erd_doc/src/ast.rs` — add `Block::Figure { path, caption, label, width }`
- `erd_doc/src/parse.rs` — detect `:figure` directive, parse key-value body
- `erd_doc/src/compile_tex.rs` — emit `\begin{figure}[htbp]` with `\includegraphics`, `\caption`, `\label`

**Design decisions:**
- `\usepackage{graphicx}` added conditionally
- `width` is fraction of `\textwidth` (default 1.0)
- `label` prefixed with `fig:` in LaTeX
- `ref`fig-energy-levels`` resolves to the figure
- Caption supports inline formatting

---

### M3: Tables

Add markdown-style table syntax.

**Syntax:**
```
:table Experimental Results
  | Parameter | Value | Unit |
  |-----------|-------|------|
  | Mass      | 1.67  | kg   |
  | Velocity  | 3.00  | m/s  |
  label: tab-results
```

**Key files:**
- `erd_doc/src/ast.rs` — add `Block::Table { title, rows, label }`
- `erd_doc/src/parse.rs` — detect `:table` directive, parse `|`-delimited rows
- `erd_doc/src/compile_tex.rs` — emit `\begin{table}[htbp]` with `tabular`

**Design decisions:**
- Second row must be separator (`|---|---|`) to mark header
- Cells support inline formatting (math, bold, etc.)
- Column alignment inferred from separator (`:---` left, `:---:` center, `---:` right)

---

### M4: Custom preamble / document class

Allow users to override the document class and add custom LaTeX packages.

**Syntax:**
```
:class revtex4-2 [aps,prl,twocolumn]
:usepackage siunitx
:usepackage pgfplots [compat=1.18]
```

**Key files:**
- `erd_doc/src/ast.rs` — add `Block::DocumentClass`, `Block::UsePackage`
- `erd_doc/src/parse.rs` — detect `:class` and `:usepackage` directives
- `erd_doc/src/compile_tex.rs` — replace default `\documentclass{article}`, append user packages to preamble

---

### M5: Multi-file include

Allow splitting a document across multiple `.erd` files.

**Syntax:**
```
:include chapters/introduction.erd
:include chapters/methods.erd
```

**Design decisions:**
- Included files are parsed and inlined at the include point
- Circular includes are detected and produce an error
- Labels and cross-references work across files
- Relative paths resolved from the including file's directory

---

### M6: Unresolved ref diagnostics

Add warnings when `ref`label`` doesn't match any section or labeled block.

**Key files:**
- `erd_doc/src/compile_tex.rs` or a new `lint.rs` — collect all labels, check all refs
- `erd_doc/src/bin/erd_lsp.rs` — emit diagnostics for unresolved refs

---

### M7: Page breaks

Add a simple `:pagebreak` directive.

**Syntax:**
```
:pagebreak
```

Compiles to `\newpage`.

---

## Implementation Notes

### M1: Document metadata (completed)

- Added `Block::Title`, `Block::Author`, `Block::Date`, `Block::Abstract` to AST
- Parser handles `:title`, `:author`, `:date` as single-line directives; `:abstract` collects indented continuation lines and parses inline fragments
- Compiler collects all metadata blocks in a first pass, emits `\title{}`, `\author{}` (joined with `\and`), `\date{}` in preamble
- `\maketitle` emitted after `\begin{document}` when any metadata present
- `\begin{abstract}...\end{abstract}` emitted with full inline formatting support
- VSCode grammar: added `directive-metadata` with patterns for all four directives
- Tests: 7 parse tests + 3 compile tests

## Verification

### M2: Figures (completed)

- Added `Block::Figure(Figure)` with `path`, `caption`, `label`, `width` fields to AST
- Parser detects `:figure path` directive, collects key-value body lines (`caption:`, `label:`, `width:`)
- Caption parsed for inline fragments (supports math, bold, etc.)
- Width defaults to 1.0 (full `\textwidth`)
- Compiler emits `\begin{figure}[htbp]` with `\centering`, `\includegraphics`, optional `\caption` and `\label{fig:...}`
- `\usepackage{graphicx}` conditionally added when figures present
- `block_has_refs` updated to check figure captions
- VSCode grammar: `directive-figure` with key-value highlighting
- Tests: 4 parse tests + 3 compile tests

### M6: Unresolved ref diagnostics (completed)

- Added `collect_labels()` and `find_unresolved_refs()` public functions in `compile_tex.rs`
- `collect_labels` gathers section slugs, figure labels, and table labels
- `find_unresolved_refs` walks all prose fragments (including nested bold/italic/footnote, lists, environments, captions) to find `Ref` nodes, then filters against known labels
- LSP `compute_diagnostics` now calls `find_unresolved_refs` and emits warnings with line-level positioning
- Tests: 4 tests (detected, resolved section, resolved figure, resolved table)

### M5: Multi-file include (completed)

- Added `resolve_includes()` function: recursively expands `:include path` lines, resolving paths relative to including file
- Added `parse_document_from_file()` entry point that resolves includes then parses
- Circular include detection via `seen` path set
- Updated `erd_compile`, `erd_check`, `erd_watch` binaries to use `parse_document_from_file`
- `erd_lsp` stays with `parse_document` (receives text from editor, not file path)
- VSCode grammar: `directive-include` rule
- Tests: 4 tests (basic, circular error, missing file error, nested includes)

### M4: Custom preamble / document class (completed)

- Added `Block::DocumentClass { name, options }` and `Block::UsePackage { name, options }` to AST
- `parse_name_with_options` helper parses `name [options]` syntax
- Parser detects `:class` and `:usepackage` directives
- Compiler uses first `:class` to override default `\documentclass{article}`, appends all `:usepackage` to preamble
- Options rendered in `[...]` for both `\documentclass` and `\usepackage`
- VSCode grammar: `directive-class` and `directive-usepackage` rules
- Tests: 6 parse tests + 3 compile tests

### M3: Tables (completed)

- Added `Block::Table(Table)` with `title`, `columns`, `header`, `rows`, `label` fields and `ColumnAlign` enum to AST
- Parser detects `:table` directive, parses pipe-delimited rows with header + separator + data rows
- Separator row determines column alignment (`:---` left, `:---:` center, `---:` right)
- Cells parsed for inline fragments (supports math, bold, etc.)
- Optional `label:` key-value line
- Compiler emits `\begin{table}[htbp]` with `\centering`, `\begin{tabular}{lcr}`, `\hline`, bold header cells
- `block_has_refs` updated to check table cells
- VSCode grammar: `directive-table` with pipe separator highlighting
- Tests: 5 parse tests + 3 compile tests

## Verification

```bash
npm test                            # full test suite
npm run compile -- file.erd         # compile and inspect LaTeX output
```
