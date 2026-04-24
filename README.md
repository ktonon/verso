# Ogma

**Verifiable Source** — a document format for writing scientific papers with machine-verified mathematics.

> **Alpha software.** Ogma is under active development and may introduce breaking changes without notice.

Ogma treats a research paper as source code: it compiles to LaTeX/PDF, and its mathematical claims are automatically verified during the build.

## Quick start

```bash
# Verify claims in a document
npm run check -- paper.ogma

# Compile to LaTeX
npm run compile -- paper.ogma

# Watch for changes and re-verify
npm run watch -- paper.ogma

# Install VSCode extension
npm run vscode:install
```

## Crates

| Crate | Purpose |
|-------|---------|
| `ogma_symbolic` | Expression parser, algebra engine, and unit system |
| `ogma_doc` | Document parser, LaTeX compiler, verifier, and LSP |
| `ogma_training` | Learned symbolic solver (ML-based simplification) |
