# Cross-References

## Goal

Add internal cross-referencing to ERD documents so authors can link to sections
(and eventually theorem-like environments) using `\hyperref` in the compiled
LaTeX output.

## Milestones

| M | Feature | Status |
|---|---------|--------|
| 1 | Section labels and `ref` tag | **completed** |

## Syntax Reference

### Section labels (automatic)

Every heading automatically generates a label from its title, slugified to
kebab-case:

```
## Earth and the Solar System
```

Compiles to:

```latex
\section{Earth and the Solar System}\label{earth-and-the-solar-system}
```

**Slug rules:** lowercase, spaces → hyphens, strip non-alphanumeric (except
hyphens), collapse consecutive hyphens, trim leading/trailing hyphens.

Examples:
- `Newton's Laws` → `newtons-laws`
- `E = mc²` → `e--mc` → `e-mc`
- `The 2nd Law` → `the-2nd-law`

### Cross-references with `ref`

Use `ref` backticks to reference a labeled section:

```
See ref`newtons-laws` for details.
```

Compiles to `\hyperref[newtons-laws]{Newton's Laws}` — the display text is
auto-resolved from the section title.

#### Custom display text

Use `|` to separate the label from custom display text:

```
ref`earth-and-the-solar-system|Hydrogen creation in planets and moons`
```

Compiles to `\hyperref[earth-and-the-solar-system]{Hydrogen creation in planets and moons}`.

#### Full example

```erd
## Earth and the Solar System

Planets form through accretion in protoplanetary disks.

## Summary of Predictions

1. **ref`earth-and-the-solar-system|Hydrogen creation in planets and moons`** *— and liquid water worlds should be abundant*
2. **ref`absolute-time|SETI is unlikely to succeed`** *— even if advanced civilizations exist*
```

Compiles to:

```latex
\section{Earth and the Solar System}\label{earth-and-the-solar-system}

Planets form through accretion in protoplanetary disks.

\section{Summary of Predictions}\label{summary-of-predictions}

\begin{enumerate}
\item \textbf{\hyperref[earth-and-the-solar-system]{Hydrogen creation in planets and moons}} \textit{--- and liquid water worlds should be abundant}
\item \textbf{\hyperref[absolute-time]{SETI is unlikely to succeed}} \textit{--- even if advanced civilizations exist}
\end{enumerate}
```

## Plan

### Phase 1: Section labels and ref tag (M1)

**Key files:**
- `verso_doc/src/ast.rs` — add `ProseFragment::Ref { label, display: Option<String> }`
- `verso_doc/src/parse.rs` — add `"ref"` to `find_tagged_backtick` tag list; parse
  `ref`label`` and `ref`label|text`` (split on first `|`)
- `verso_doc/src/compile_tex.rs`:
  - Add `slugify(title) -> String` function
  - Headings emit `\label{slug}` after `\section{Title}`
  - Build section label→title map for auto-resolving ref display text
  - `Ref` compiles to `\hyperref[label]{text}`
  - Add `\usepackage{hyperref}` to preamble when refs are present
- `verso_doc/src/report.rs` — handle `Ref` in `prose_to_string` if needed

**Design decisions:**
- No `sec:` prefix on labels — matches user's LaTeX convention directly
- `\usepackage{hyperref}` only added when document contains `ref` tags
- Unresolved refs (label doesn't match any section) use the label as display text
  with a compiler warning (not an error — the label may reference a section in
  another file or a manually-placed label)
- `|` is the separator because it doesn't appear in slugs or typical display text

**Tests:**
- `slugify` unit tests (spaces, apostrophes, special chars, consecutive hyphens)
- Parse `ref`label`` → `Ref { label: "label", display: None }`
- Parse `ref`label|custom text`` → `Ref { label: "label", display: Some("custom text") }`
- Compile heading with auto-label
- Compile ref with auto-resolved title
- Compile ref with custom display text
- Ref inside bold inside list item (the motivating example)
- `\usepackage{hyperref}` appears in preamble when refs used

**Estimated scope:** ~40 lines parser, ~5 lines AST, ~50 lines compile_tex, ~20 lines tests.

## Implementation Notes

### Phase 1: Section labels and ref tag (completed)

- Added `ProseFragment::Ref { label, display: Option<String> }` to AST
- `ref` added to `find_tagged_backtick` tag list; `|` splits label from display text
- `compile_tex.rs` adds `slugify()` function: lowercase, strip non-alnum, hyphens for spaces, collapse consecutive hyphens
- Sections now emit `\label{slug}` after `\section{Title}`
- `\usepackage{hyperref}` conditionally added when any `Ref` fragments exist
- Compiler builds section label→title map; `ref`label`` auto-resolves to section title
- Unresolved labels fall back to using the label string as display text
- `write_prose_fragments` and callers now accept `&HashMap<String, String>` for title resolution
- 4 parse tests + 7 compile tests (including slugify unit tests)

## Verification

```bash
cargo test --package verso_doc
```
