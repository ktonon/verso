# Unicode Completions

## Goal

Make it easy to type Greek letters, mathematical operators, and other special characters in both the REPL and VSCode. This supports the shift toward more symbolic notation (derivatives, vector operators, etc.) needed for ERD and future papers.

## Design

### Trigger: `:name:`

The user types `:name:` (e.g., `:mu:`, `:partial:`, `:nabla:`) and the text is replaced by the corresponding unicode character. This follows the same convention as GitHub, Slack, and Discord.

- In the **REPL**, replacement happens on submit (the input line is scanned for `:name:` patterns before evaluation).
- In **VSCode**, a completion popup appears when `:` is typed, filtered by what follows. Selecting a completion inserts the character and removes the trigger text.

### No conflict with REPL commands

REPL commands and document directives use `!` as their prefix (`!var`, `!const`, `!claim`, etc.). Unicode triggers use *paired* colons (`:name:`). There is no ambiguity.

### LaTeX transpilation

Verso source files contain literal unicode characters (e.g., `μ`, `∂`, `∇`). When transpiling to LaTeX, these must be converted to the appropriate LaTeX commands:

| Verso source | LaTeX output |
|-------------|-------------|
| `μ` | `\mu` |
| `∂f/∂x` | `\partial f / \partial x` |
| `∇ · F` | `\nabla \cdot F` |

The unicode table stores a triple: `(name, char, latex)`. Most names match the LaTeX command directly (mu → `\mu`, alpha → `\alpha`), so the LaTeX string can default to `\{name}` with explicit overrides only where they diverge (e.g., `inf` → `\infty`, `cdot` → `\cdot`).

The `to_tex` module currently renders `Var { name }` as the name verbatim. It needs to look up unicode characters and emit the LaTeX equivalent. This applies to variable names, function arguments, and any free text in math expressions.

### Character set

Start with the characters most useful for mathematical physics:

**Greek lowercase**: alpha (α), beta (β), gamma (γ), delta (δ), epsilon (ε), zeta (ζ), eta (η), theta (θ), iota (ι), kappa (κ), lambda (λ), mu (μ), nu (ν), xi (ξ), pi (π), rho (ρ), sigma (σ), tau (τ), upsilon (υ), phi (φ), chi (χ), psi (ψ), omega (ω)

**Greek uppercase**: Gamma (Γ), Delta (Δ), Theta (Θ), Lambda (Λ), Xi (Ξ), Pi (Π), Sigma (Σ), Phi (Φ), Psi (Ψ), Omega (Ω)

**Math operators**: partial (∂), nabla (∇), inf/infinity (∞), sqrt (√), sum (∑), prod (∏), integral (∫), pm (±), mp (∓), times (×), cdot (·), leq (≤), geq (≥), neq (≠), approx (≈), equiv (≡), in (∈), notin (∉), subset (⊂), supset (⊃), forall (∀), exists (∃), hbar (ℏ)

**Arrows**: to/rightarrow (→), leftarrow (←), leftrightarrow (↔), implies (⇒), iff (⇔), mapsto (↦)

Extensible — new entries can be added to the table without code changes.

## Plan

### Shared unicode table

- New module `verso_symbolic/src/unicode.rs`
- Table entry: `(name: &str, char: char, latex: &str)`
- `pub fn lookup(name: &str) -> Option<char>` — name → unicode char
- `pub fn to_latex(c: char) -> Option<&str>` — unicode char → LaTeX command
- `pub fn completions(prefix: &str) -> Vec<(&str, char)>` — prefix search for popup
- `pub fn replace_all(input: &str) -> String` — scan for `:name:` patterns, replace matches
- Table is a static `&[UnicodeEntry]` sorted by name

### REPL integration

- In `Session::eval`, call `replace_all` on the input before any other processing
- Also call it in the `run()` readline loop so the prompt echo shows the replaced text

### LaTeX integration

- In `to_tex.rs`, use `to_latex(c)` when rendering variable names, function names, and text containing unicode math symbols
- Applies to `ExprKind::Var`, `ExprKind::Fn(Custom(...))`, and any string output in math mode

### VSCode / LSP integration

- Add `CompletionProvider` capability to the LSP server
- Trigger character: `:`
- On trigger, read back to the previous `:` to determine the prefix
- Return `CompletionItem`s with `insertText` set to the unicode character and `range` covering the full `:prefix` text
- `filterText` set to `:name:` so typing narrows the list
- Each item includes a `detail` field showing the character name and code point

### Future enhancements

- **Tab completion in REPL**: rustyline `Completer` trait for `:name` + Tab
- **Character palette in VSCode**: custom webview panel showing all available characters, clickable
- **Aliases**: multiple names for the same character (e.g., `:eps:` and `:epsilon:` both → ε)
- **Custom user mappings**: user-defined completions in a config file

## Implementation Notes

Not yet started.

## Verification

- `cargo test --release -p verso_symbolic -- unicode` exercises the lookup/replace/to_latex functions
- REPL e2e tests verify `:mu:` → `μ` replacement in expressions
- LaTeX output tests verify `μ` → `\mu` in generated `.tex` files
- Manual verification of VSCode completion popup behavior
