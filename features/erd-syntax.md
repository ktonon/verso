# ERD Markup Syntax

## Goal

Define and implement the full ERD markup language — a document format for writing
physics papers with machine-verified mathematics. ERD borrows from markdown where
conventions align, but is its own language, not a superset.

## Milestones

| M | Feature | Status |
|---|---------|--------|
| 1 | Inline formatting: bold, italic | **completed** |
| 2 | Lists: bullet and numbered | **completed** |
| 3 | Multiline math blocks | **completed** |
| 4 | Citations and bibliography | **completed** |
| 5 | Theorem-like environments | **completed** |
| 6 | Block quotes, footnotes, comments | **completed** |

## Syntax Reference

This section documents the intended ERD syntax, both what exists today and what
is planned. Items marked **(planned)** are not yet implemented.

---

### Document structure

An ERD document is a sequence of blocks separated by blank lines. Blocks are
either structural (headings, directives) or content (prose, math, claims, proofs).

---

### Document metadata

Front matter directives define the document's title, authors, date, and abstract.
These can appear anywhere in the source but always compile to the LaTeX preamble
and front matter.

```
:title Quantum Corrections to the Classical Limit
:author Alice Smith
:author Bob Jones
:date 2026-03-13
:abstract
  We present a novel approach to computing quantum corrections
  in the semiclassical regime.
```

| Directive | LaTeX output | Notes |
|-----------|-------------|-------|
| `:title text` | `\title{text}` | Required for `\maketitle` |
| `:author name` | `\author{name}` | Multiple authors joined with `\and` |
| `:date text` | `\date{text}` | Optional; omitted → no date |
| `:abstract` | `\begin{abstract}...\end{abstract}` | Indented body, supports inline formatting |

When any of `:title`, `:author`, or `:date` is present, `\maketitle` is emitted
after `\begin{document}`.

---

### Headings

```
# Top-level section
## Subsection
### Sub-subsection
#### Paragraph heading
```

Identical to markdown. Compiles to `\section`, `\subsection`, `\subsubsection`,
`\paragraph` in LaTeX.

---

### Prose

Consecutive non-blank lines that don't start with `#` or `:` form a prose
paragraph. Lines are joined with spaces.

```
This is a paragraph of prose. It continues on the next
line until a blank line or a directive.
```

Compiles to a LaTeX paragraph (plain text block).

---

### Inline formatting

Within prose, the following inline constructs are recognized:

| Syntax | Meaning | LaTeX output |
|--------|---------|--------------|
| `math`expr`` | Inline math (parsed) | `$\text{to\_tex(expr)}$` |
| `tex`raw`` | Inline raw LaTeX | `$raw$` |
| `claim`name`` | Equation reference | `\eqref{eq:name}` |
| `cite`key`` | Bibliography citation | `\cite{key}` |
| `ref`label`` | Section cross-reference | `\hyperref[label]{Title}` |
| `ref`label\|text`` | Cross-reference with display text | `\hyperref[label]{text}` |
| `url`https://...`` | URL link | `\url{https://...}` |
| `url`https://...\|text`` | URL with display text | `\href{https://...}{text}` |
| `**text**` | Bold | `\textbf{text}` |
| `*text*` | Italic | `\textit{text}` |

#### Inline math

Use `math` backticks for expressions that ERD can parse, verify, and render:

```
The identity math`sin(x)^2 + cos(x)^2` equals one.
```

The expression is parsed by erd_symbolic, so it uses expression syntax (not LaTeX).
It compiles to LaTeX via the `ToTex` trait.

#### Raw LaTeX

Use `tex` backticks for LaTeX that doesn't need to be parsed:

```
The vector tex`\vec{v}` has magnitude tex`|\vec{v}|`.
```

Passed through verbatim inside `$...$`.

#### Claim references

Use `claim` backticks to reference a named claim:

```
By claim`pythagorean`, we know that...
```

Compiles to `\eqref{eq:pythagorean}`, producing a clickable "(1)" style reference.

#### Citations 
Use `cite` backticks to cite a bibliography entry:

```
As shown by cite`einstein1905`, the energy is...
```

Compiles to `\cite{einstein1905}`.

#### Bold and italic 
```
This is **bold** and this is *italic*.
```

Compiles to `\textbf{bold}` and `\textit{italic}`. Nesting is not supported.
Bold-italic can be written as `***text***` **(planned)**.

#### Prose escaping

Certain characters in prose text are automatically converted for correct LaTeX output:

| ERD source | LaTeX output | Notes |
|------------|-------------|-------|
| `~` | `\textasciitilde{}` | Tilde (not a non-breaking space) |
| `"text"` | `` ``text'' `` | Smart quotes (paired `"` converted) |

Unpaired `"` (odd count in a text fragment) leaves the last `"` as-is.
Both escapes apply only within `ProseFragment::Text` — not inside math, tex, or other tags.

#### Cross-references

Use `ref` backticks to reference a section by its auto-generated label:

```
See ref`newtons-laws` for details.
```

The label is the slugified section title (lowercase, hyphens for spaces, special
characters stripped). The display text is auto-resolved from the section title.

For custom display text, use `|`:

```
ref`earth-and-the-solar-system|Hydrogen creation in planets and moons`
```

Compiles to `\hyperref[earth-and-the-solar-system]{Hydrogen creation in planets and moons}`.

Headings automatically generate labels:

```
## Newton's Laws     →  \label{newtons-laws}
## The 2nd Law       →  \label{the-2nd-law}
```

---

### Claims

A claim is a named equation that ERD verifies. Verification checks that
`simplify(lhs - rhs) == 0`, falling back to numerical spot-check if symbolic
simplification doesn't reach zero.

```
:claim pythagorean
  sin(x)^2 + cos(x)^2 = 1
```

- The body must be indented.
- Exactly one `=` separates the left-hand side from the right-hand side.
- Both sides use erd_symbolic expression syntax.
- Compiles to a numbered LaTeX equation with `\label{eq:name}`.

---

### Proofs

A proof is a step-by-step derivation that references a claim. Each adjacent pair
of steps is verified. Justifications are optional annotations after `;`.

```
:proof pythagorean
  sin(x)^2 + cos(x)^2
  = 1 - cos(x)^2 + cos(x)^2      ; sin_sq
  = 1                              ; add_cancel
```

- The first step has no leading `=`.
- Subsequent steps begin with `=` (stripped by the parser).
- Justifications after `;` name a rule. If a named rule exists in `RuleSet::full()`,
  ERD first checks whether that specific rule transforms the previous step into the
  current one.
- Compiles to a LaTeX `align*` environment.

---

### Dimension declarations

Declare the physical dimensions of a variable. When present, ERD checks that both
sides of every claim have matching dimensions.

```
:dim v [L T^-1]
:dim m [M]
:dim F [M L T^-2]
```

- Bracket notation: base dimensions separated by spaces, exponents via `^`.
- Base dimensions: `L` (length), `M` (mass), `T` (time), `Theta` or `\u{0398}` (temperature),
  `I` (current), `N` (amount), `J` (luminosity).
- Dimensionless: `[1]`.
- Dimension declarations produce no LaTeX output.

---

### Multiline math blocks 
For displayed math that isn't a claim or proof — derivations, definitions, or
sequences of related equations that don't need verification:

````
```math
E = mc^2
p = mv
```
````

- Each line is a separate expression, parsed by erd_symbolic.
- Compiles to a LaTeX `gather*` (un-numbered) or `align*` environment.
- Not verified — use `:claim` for equations that should be checked.

A `math` block with alignment (using `&`) compiles to `align*`:

````
```math
F &= ma
  &= m \frac{dv}{dt}
```
````

---

### Lists 
#### Bullet lists

```
- First item
- Second item
- Third item with math`x^2` inline
```

Nesting via indentation:

```
- Outer item
  - Nested item
  - Another nested item
- Back to outer
```

Compiles to `\begin{itemize}` / `\item` / `\end{itemize}`.

#### Numbered lists

```
1. First step
2. Second step
3. Third step
```

Compiles to `\begin{enumerate}` / `\item` / `\end{enumerate}`.

Inline formatting (bold, italic, math, citations) works inside list items.

---

### Citations and bibliography 
#### Declaring a bibliography

```
:bibliography refs.bib
```

Points to a BibTeX file. Compiles to `\bibliography{refs}` with
`\bibliographystyle{plain}` (configurable).

#### Citing

```
As shown in cite`einstein1905`, the photoelectric effect...
```

Compiles to `\cite{einstein1905}`. Multiple keys: `cite`einstein1905,dirac1928``.

---

### Figures

Include images with optional captions, labels, and width control:

```
:figure plots/energy-levels.pdf
  caption: Energy levels of the hydrogen atom.
  label: fig-energy-levels
  width: 0.8
```

| Property | Default | Notes |
|----------|---------|-------|
| `caption:` | (none) | Supports inline formatting (math, bold, etc.) |
| `label:` | (none) | Prefixed with `fig:` in LaTeX |
| `width:` | `1.0` | Fraction of `\textwidth` |

Compiles to `\begin{figure}[htbp]` with `\includegraphics`, `\caption`, `\label`.
`\usepackage{graphicx}` is added conditionally when figures are present.

---

### Table of contents

```
:toc
```

Compiles to `\tableofcontents`.

---

### Page breaks

```
:pagebreak
```

Compiles to `\newpage`.

---

### Multi-file include

Split a document across multiple `.erd` files:

```
:include chapters/introduction.erd
:include chapters/methods.erd
```

- Included files are parsed and inlined at the include point.
- Paths are relative to the including file's directory.
- Nested includes are supported (included files can include other files).
- Circular includes are detected and produce an error.
- Labels and cross-references work across included files.

---

### Default preamble

The compiler automatically generates a LaTeX preamble with sensible defaults:

- `\documentclass[11pt]{article}` with `geometry`, `fontenc`, `inputenc`, `lmodern`, `microtype`
- Math support: `amsmath`, `amsthm`
- Styling: `xcolor`, `framed`, `bookmark`
- `hyperref` (with colored links) is included when the document uses `ref` or `url` tags
- `graphicx` and `wrapfig` are included when the document contains figures
- Layout: no paragraph indent, 6pt paragraph skip, 3-level TOC depth

Documents do not need to specify packages — erd chooses reasonable defaults.

---

### Tables

Tables use the `:table` directive with pipe-delimited rows:

```
:table Experimental Results
  | Parameter | Value | Unit |
  |-----------|-------|------|
  | Mass      | 1.67  | kg   |
  | Velocity  | 3.00  | m/s  |
  label: tab-results
```

- The second row must be a separator (`|---|---|`) marking the header boundary.
- Column alignment inferred from separator: `:---` left, `:---:` center, `---:` right. Default is left.
- Cells support inline formatting (math, bold, etc.).
- Optional `label:` line (prefixed with `tab:` in LaTeX).
- Optional title after `:table` becomes `\caption{}`.
- Compiles to `\begin{table}[htbp]` with `tabular`.

---

### Theorem-like environments
For theorems, lemmas, definitions, corollaries, and remarks:

```
:theorem Noether's Theorem
  Every differentiable symmetry of the action of a physical system
  has a corresponding conservation law.

:definition Conservative Force
  A force is conservative if the work done moving a particle between
  two points is independent of the path taken.

:lemma orthogonality
  If math`\langle u, v \rangle = 0`, then math`u` and math`v` are
  linearly independent.
```

Supported environments: `theorem`, `lemma`, `definition`, `corollary`, `remark`,
`example`.

Compiles to `\begin{theorem}[Noether's Theorem]` ... `\end{theorem}` with
appropriate `\newtheorem` declarations in the preamble.

---

### Block quotes 
```
> This is a block quote, useful for stating results from other sources
> or providing extended remarks.
```

Compiles to `\begin{quote}` ... `\end{quote}`.

---

### Footnotes 
```
This result is surprising^[Though it was anticipated by Euler in 1748.].
```

The `^[...]` syntax places a footnote. Compiles to `\footnote{...}`.

---

### Comments

```
% This line is a comment and will not appear in the output.
```

Lines starting with `%` are ignored by the parser and produce no output.
Consistent with LaTeX comment convention.

---

### Expression syntax

Expressions inside `math` backticks, claims, and proofs use erd_symbolic syntax,
not LaTeX. Key differences from LaTeX:

| ERD expression | LaTeX equivalent | Notes |
|---------------|-----------------|-------|
| `x^2` | `x^{2}` | Power (exponent) |
| `x^(a+b)` | `x^{a+b}` | Parenthesized exponent |
| `T^{mu}` | `T^{\mu}` | Tensor upper index (superscript) |
| `T_{mu}` | `T_{\mu}` | Tensor lower index (subscript) |
| `sin(x)` | `\sin{x}` | Functions |
| `pi` or `\u{03C0}` | `\pi` | Pi constant |
| `sqrt(x)` | `\sqrt{x}` | Square root |
| `x * y` or `xy` | `xy` | Implicit multiplication |
| `2*10^8` | `2 \times 10^{8}` | Numeric × numeric uses `\times` |
| `1_000_000` | `1000000` | Underscores as visual separators |
| `1/2` | `\frac{1}{2}` | Fractions |
| `exp(x)` | `e^{x}` | Exponential |

`^` means exponent by default. `^{...}` with curly braces means tensor index
(superscript).

The `ToTex` trait handles conversion to proper LaTeX notation.

#### Dimension and unit annotations

Square brackets `[...]` after an expression annotate it with physical dimensions
or SI units. The meaning depends on context:

| Syntax | Context | Meaning |
|--------|---------|---------|
| `v [L T^-1]` | Variable | Dimension annotation |
| `F [M L T^-2]` | Variable | Dimension annotation |
| `theta [1]` | Variable | Dimensionless annotation |
| `3 [m]` | Numeric | Unit annotation → `Quantity` |
| `5 [km]` | Numeric | Prefixed unit → `Quantity` |
| `3*10^8 [m/s]` | Numeric | Compound unit → `Quantity` |
| `10 [kg*m/s^2]` | Numeric | Same as `[N]` → `Quantity` |
| `100 [N]` | Numeric | Derived SI unit → `Quantity` |
| `5 [1/s]` | Numeric | Inverse unit → `Quantity` |
| `c [m/s]` | Variable + unit | **SYNTAX ERROR** |
| `3 [L]` | Numeric + dimension | **SYNTAX ERROR** |

**Variables require dimensions** (uppercase base dimension symbols: L, M, T,
Theta, I, N, J). **Numeric values require units** (lowercase/mixed SI symbols:
m, s, kg, N, Hz, Pa, etc.).

Dimension shorthand: `[L/T]` is equivalent to `[L T^-1]`.

**SI base units:** m (meter), g (gram), s (second), K (kelvin), A (ampere),
mol (mole), cd (candela). Note: gram (not kilogram) is the parseable base;
`kg` = k prefix (10³) × g (10⁻³) = scale 1.0.

**Derived units:** N (newton), J (joule), W (watt), Pa (pascal), Hz (hertz),
C (coulomb), V (volt), Ohm (ohm).

**SI prefixes:** p (pico, 10⁻¹²), n (nano, 10⁻⁹), μ (micro, 10⁻⁶),
m (milli, 10⁻³), c (centi, 10⁻²), k (kilo, 10³), M (mega, 10⁶),
G (giga, 10⁹), T (tera, 10¹²).

Quantities are evaluated by converting to base SI: `eval(5 [km]) = 5000.0`.
Dimensional analysis uses the unit's implied dimension:
`infer_dim(3 [m/s]) = [L T⁻¹]`.

---

## What ERD is not

- **Not a superset of markdown.** ERD borrows `#` headings, prose paragraphs,
  `**bold**`, `*italic*`, and list syntax from markdown. Other markdown features
  (HTML blocks, reference links, tables, images) are not supported.
- **Not LaTeX.** The source format is designed to be readable and writable without
  LaTeX knowledge. LaTeX is a compilation target.
- **Not a general-purpose document format.** ERD is purpose-built for physics and
  mathematics papers with verified claims.

## Implementation Notes

### Phase 1: Inline formatting (completed)

- Added `Bold(Vec<ProseFragment>)` and `Italic(Vec<ProseFragment>)` to `ProseFragment` enum
- `parse_prose_fragments` now finds the earliest inline construct (tagged backtick or emphasis marker) and processes it, with recursive parsing for inner content
- `***text***` produces `Bold([Italic([Text("text")])])` — bold wraps italic
- `find_emphasis` handles `**` before `*` to avoid false matches; skips lone `*` that are part of `**`
- `compile_tex.rs` refactored: extracted `write_prose_fragments` for recursive rendering of `\textbf{}` / `\textit{}` nesting
- 7 new tests (5 parse, 2 compile)

### Phase 2: Lists (completed)

- Added `List { ordered, items, span }` and `ListItem { fragments, children }` to AST
- Parser detects `- ` (bullet) and `N. ` (ordered) markers at block level
- Nesting via indentation: deeper-indented markers become children of the previous item
- List terminates on blank line, heading, directive, or outdented non-marker line
- Items support full inline formatting (bold, italic, math, etc.)
- LaTeX: `\begin{itemize}` / `\begin{enumerate}` with recursive `write_list`
- 6 new tests (bullet, numbered, nested, inline math, termination by blank/directive)

### Phase 3: Multiline math blocks (completed)

- Added `MathBlock { exprs, span }` to AST
- Parser detects ` ```math ` opening fence, collects lines until closing ` ``` `, parses each non-empty line as an expression
- Unclosed blocks produce a parse error
- Single-expression blocks compile to `\[ ... \]`, multi-expression to `\begin{gather*}`
- Math blocks are not verified — they're display-only (verifier skips `MathBlock`)
- 7 new tests (5 parse, 2 compile)

### Phase 4: Citations and bibliography (completed)

- Added `ProseFragment::Cite(Vec<String>)` and `Block::Bibliography { path, span }` to AST
- `cite` added to `find_tagged_backtick` tag list; comma-separated keys parsed
- `:bibliography path.bib` directive parsed at block level
- Bibliography output placed at end of document (before `\end{document}`)
- `.bib` extension stripped from path in `\bibliography{}` output
- 6 new tests (4 parse, 2 compile)

### Phase 5: Theorem-like environments (completed)

- Added `Environment { kind, title, body, span }` and `EnvKind` enum (6 variants) to AST
- Parser detects `:theorem`, `:lemma`, `:definition`, `:corollary`, `:remark`, `:example` directives
- Title is optional text after the directive keyword; body is indented continuation lines parsed for inline fragments
- `compile_tex.rs` collects used `EnvKind`s and emits `\newtheorem` declarations in the preamble (once per kind)
- Environments compile to `\begin{theorem}[Title]` ... `\end{theorem}` (or without `[Title]` when none given)
- `\usepackage{amsthm}` added to preamble
- 7 new parse tests + 5 new compile tests

### Phase 6: Block quotes, footnotes, comments (completed)

- Added `Block::BlockQuote(Vec<ProseFragment>)` and `ProseFragment::Footnote(Vec<ProseFragment>)` to AST
- Comments: lines starting with `%` are skipped early in the main parsing loop (before any block detection)
- Block quotes: consecutive lines starting with `> ` are collected and parsed for inline fragments; compiles to `\begin{quote}` ... `\end{quote}`
- Footnotes: `^[text]` inline construct detected by `find_footnote` with bracket nesting support; inner content recursively parsed for inline formatting; compiles to `\footnote{text}`
- Prose termination updated to stop on `%`, `> `, and ` ```math ` lines
- 10 new parse tests + 3 new compile tests

## Plan

### Phase 1: Inline formatting (M1)

Extend `ProseFragment` and the prose parser to handle bold and italic.

**Key files:**
- `erd_doc/src/ast.rs` — add `Bold(Vec<ProseFragment>)`, `Italic(Vec<ProseFragment>)` variants
- `erd_doc/src/parse.rs` — extend `parse_prose_fragments` to detect `**...**` and `*...*`
  - Parse outermost delimiters first; inner content is recursively parsed for nested tags
  - `***text***` produces `Bold([Italic([Text("text")])])`
  - Edge case: `*` inside `math` backticks must not trigger italic (tags take precedence)
- `erd_doc/src/compile_tex.rs` — `Bold` → `\textbf{...}`, `Italic` → `\textit{...}`
- `erd_doc/src/report.rs` — may need minor update for fragment display

**Tests:**
- Parse `**bold**` in prose → `Bold([Text("bold")])`
- Parse `*italic*` in prose → `Italic([Text("italic")])`
- Parse `**bold with math`x`**` → `Bold([Text("bold with "), Math(x)])`
- Parse `***both***` → `Bold([Italic([Text("both")])])`
- Compile to LaTeX and verify `\textbf`, `\textit` output
- Existing tests remain green (no regressions in tag parsing)

**Estimated scope:** ~80 lines parser, ~10 lines AST, ~15 lines compile_tex.

---

### Phase 2: Lists (M2)

Add bullet and numbered list blocks to the parser.

**Key files:**
- `erd_doc/src/ast.rs` — add `Block::List { ordered: bool, items: Vec<ListItem> }` and
  `ListItem { fragments: Vec<ProseFragment>, children: Option<List> }` for nesting
- `erd_doc/src/parse.rs` — detect lines starting with `- ` or `N. ` at the block level;
  collect consecutive list lines; determine nesting by indentation depth (2-space increments)
  - A list terminates on blank line, heading, or directive
  - List items support inline formatting (bold, italic, math, cite, etc.)
- `erd_doc/src/compile_tex.rs` — `itemize`/`enumerate` environments with `\item`

**Design decisions:**
- Mixed ordered/unordered at the same level is an error
- Nesting up to 3 levels (LaTeX limitation for default counters)
- Continuation lines (indented non-marker lines) append to the current item

**Tests:**
- Simple bullet list → 3 items
- Simple numbered list → 3 items
- Nested bullet list → outer with inner children
- List with inline math → fragments parsed correctly
- List terminated by blank line, heading, and directive
- Compile to LaTeX and verify `\begin{itemize}` structure

**Estimated scope:** ~120 lines parser, ~20 lines AST, ~30 lines compile_tex.

---

### Phase 3: Multiline math blocks (M3)

Add fenced `math` blocks for displayed equations that don't need verification.

**Key files:**
- `erd_doc/src/ast.rs` — add `Block::MathBlock { exprs: Vec<Expr>, aligned: bool }`
- `erd_doc/src/parse.rs` — detect ` ```math ` opening fence; collect lines until closing
  ` ``` `; parse each line as an expression via `parse_expr`
  - Lines containing `&` set `aligned: true` (for `align*` output)
  - Empty lines within the block are preserved as visual breaks
- `erd_doc/src/compile_tex.rs` — `aligned` → `\begin{align*}`, otherwise `\begin{gather*}`

**Design decisions:**
- Math blocks are not verified (no claim name, no pass/fail)
- Each non-empty line is one expression
- The verifier skips `MathBlock` entirely
- Future: could allow optional labeling for cross-reference

**Tests:**
- Parse single-expression math block
- Parse multi-expression math block
- Compile to `gather*` (no alignment)
- Compile to `align*` (with `&`)
- Verify that math blocks don't appear in verification results

**Estimated scope:** ~40 lines parser, ~5 lines AST, ~20 lines compile_tex.

---

### Phase 4: Citations and bibliography (M4)

Add `cite` inline tag and `:bibliography` directive.

**Key files:**
- `erd_doc/src/ast.rs` — add `ProseFragment::Cite(Vec<String>)` (multiple keys),
  `Block::Bibliography { path: String }`
- `erd_doc/src/parse.rs` — add `"cite"` to the tag list in `find_tagged_backtick`;
  parse comma-separated keys; detect `:bibliography` directive
- `erd_doc/src/compile_tex.rs` — `Cite` → `\cite{key1,key2}`;
  `Bibliography` → `\bibliographystyle{plain}\n\bibliography{path}`
  (strip `.bib` extension from path)

**Design decisions:**
- No built-in BibTeX validation — that's `pdflatex`/`bibtex`'s job
- `:bibliography` can appear anywhere but compiles at end of document (before `\end{document}`)
- Multiple `:bibliography` directives are an error

**Tests:**
- Parse `cite`key`` → `Cite(["key"])`
- Parse `cite`a,b,c`` → `Cite(["a", "b", "c"])`
- Parse `:bibliography refs.bib` → `Bibliography { path: "refs.bib" }`
- Compile cite to `\cite{key}`
- Compile bibliography to `\bibliography{refs}`

**Estimated scope:** ~30 lines parser, ~5 lines AST, ~15 lines compile_tex.

---

### Phase 5: Theorem-like environments (M5)

Add `:theorem`, `:definition`, `:lemma`, `:corollary`, `:remark`, `:example` directives.

**Key files:**
- `erd_doc/src/ast.rs` — add `Block::Environment { kind: EnvKind, title: Option<String>, body: Vec<ProseFragment> }`
  and `EnvKind` enum
- `erd_doc/src/parse.rs` — detect `:theorem`, `:definition`, etc. at block level;
  title is the rest of the directive line; body is indented continuation lines
  (parsed for inline fragments)
- `erd_doc/src/compile_tex.rs` — emit `\newtheorem` declarations in preamble (once per
  kind used); emit `\begin{theorem}[Title]` ... `\end{theorem}`

**Design decisions:**
- Body supports full inline formatting (math, bold, italic, cite)
- Theorems are not verified — they're prose containers, not claims
- If a theorem contains a verifiable statement, the author should also write a `:claim`
- Numbering handled by LaTeX's `\newtheorem` counter

**Tests:**
- Parse `:theorem Name` with indented body
- Parse `:definition` without title
- Body contains inline math
- Compile to `\begin{theorem}[Name]` with `\newtheorem` in preamble
- Multiple environment types in one document

**Estimated scope:** ~50 lines parser, ~15 lines AST, ~40 lines compile_tex.

---

### Phase 6: Block quotes, footnotes, and comments (M6)

Add remaining prose-level constructs.

**Key files:**
- `erd_doc/src/ast.rs` — add `Block::BlockQuote(Vec<ProseFragment>)`,
  `ProseFragment::Footnote(Vec<ProseFragment>)`
- `erd_doc/src/parse.rs`:
  - Block quotes: lines starting with `> ` collected into a block; content parsed for fragments
  - Footnotes: detect `^[` in prose fragment parsing; find matching `]`; recursively parse inner content
  - Comments: lines starting with `%` skipped entirely (before any other block detection)
- `erd_doc/src/compile_tex.rs` — `BlockQuote` → `\begin{quote}...\end{quote}`,
  `Footnote` → `\footnote{...}`

**Design decisions:**
- Block quotes are single-level (no nested `>>`)
- Footnotes cannot contain other footnotes
- Comments are line-level only (no inline `% rest of line`)

**Tests:**
- Parse `> quoted text` → `BlockQuote`
- Multi-line block quote (consecutive `>` lines)
- Parse `text^[note]more` → `[Text, Footnote([Text]), Text]`
- Parse `% comment` → skipped
- Compile block quote and footnote to LaTeX

**Estimated scope:** ~60 lines parser, ~5 lines AST, ~15 lines compile_tex.

---

### Phase ordering rationale

Phases 1-2 (inline formatting, lists) are highest value — they unblock writing
real prose-heavy papers. Phase 3 (math blocks) fills the gap between "everything
must be a claim" and "use raw LaTeX." Phases 4-5 (citations, theorems) add
academic paper infrastructure. Phase 6 (quotes, footnotes, comments) rounds out
the language.

Each phase is independently shippable and testable. Later phases don't depend on
earlier ones (except that inline formatting from Phase 1 is used in list items,
theorem bodies, etc. — so Phase 1 should land first).

## Verification

```bash
npm run check -- file.erd           # verify claims and dimensions
npm run compile -- file.erd         # compile to LaTeX
npm run watch -- file.erd           # re-verify on save
npm test                            # full test suite
```
