# Unicode Completions

## Goal

Make it easy to type Greek letters, mathematical operators, and other special characters in both the REPL and VSCode. This supports the shift toward more symbolic notation (derivatives, vector operators, etc.) needed for ERD and future papers.

## Design

### Trigger: `:name:`

The user types `:name:` (e.g., `:mu:`, `:partial:`, `:nabla:`) and the text is replaced by the corresponding unicode character. This follows the same convention as GitHub, Slack, and Discord.

- In the **REPL**, replacement happens on submit (the input line is scanned for `:name:` patterns before evaluation).
- In **VSCode**, a completion popup appears when `:` is typed, filtered by what follows. Selecting a completion inserts the character and removes the trigger text.

### No conflict with REPL commands

REPL commands (`:var`, `:const`, `:func`, `:reset`, etc.) use a single colon at the start of the line. Unicode triggers use *paired* colons (`:name:`), so there is no ambiguity.

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
- `pub fn lookup(name: &str) -> Option<char>` — exact match
- `pub fn completions(prefix: &str) -> Vec<(&str, char)>` — prefix search for popup
- `pub fn replace_all(input: &str) -> String` — scan for `:name:` patterns, replace matches
- Table is a static `&[(&str, char)]` sorted by name

### REPL integration

- In `Session::eval`, call `replace_all` on the input before any other processing
- Also call it in the `run()` readline loop so the prompt echo shows the replaced text

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

- `cargo test --release -p verso_symbolic -- unicode` exercises the lookup/replace functions
- REPL e2e tests verify `:mu:` → `μ` replacement in expressions
- Manual verification of VSCode completion popup behavior
