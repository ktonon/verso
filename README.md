# Verso

**Verifiable Source** — a document format for writing scientific papers with machine-verified mathematics.

Verso treats a research paper as source code: it compiles to LaTeX/PDF, and its mathematical claims are automatically verified during the build.

## Quick start

```bash
# Verify claims in a document
npm run check -- paper.verso

# Compile to LaTeX
npm run compile -- paper.verso

# Watch for changes and re-verify
npm run watch -- paper.verso

# Install VSCode extension
npm run vscode:install
```

## Crates

| Crate | Purpose |
|-------|---------|
| `verso_symbolic` | Expression parser, algebra engine, and unit system |
| `verso_doc` | Document parser, LaTeX compiler, verifier, and LSP |
| `verso_training` | Learned symbolic solver (ML-based simplification) |
