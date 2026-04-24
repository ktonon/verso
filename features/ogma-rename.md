# Ogma Rename

## Goal

Rebrand the project from **erd** (Emergent Rung Dynamics) to **ogma** (Verifiable Source). The project has evolved from a physics simulation into a paper-writing tool with machine-verified math. The new name reflects its actual purpose: treating scientific papers as verifiable source code.

This rename also cleans up components that no longer belong in a paper-writing tool.

## Plan

### Phase 0: Remove non-paper components

These components predate the paper-writing pivot and are no longer part of ogma's mission.

**Remove entirely:**

| Component | What it is | Why remove |
|-----------|-----------|------------|
| `erd_model/` | 3D globe math model (nalgebra) | Physics simulation, not paper writing |
| `erd_viewer/` | WASM 3D globe renderer | Visualization, not paper writing |
| `erd_app/` | Frontend TypeScript app for viewer | Viewer frontend |
| `infrastructure/` | AWS CDK deployment | Deployed the viewer app |
| `scripts/build-app.js` | Viewer app build script | Viewer build |
| `scripts/config.js` | Viewer app config | Viewer config |
| `public/` | Static assets (compiled WASM) | Viewer assets |
| `dist/` | Compiled viewer output | Viewer artifacts |

**Keep (related to learned symbolic solver):**

| Component | What it is | Why keep |
|-----------|-----------|----------|
| `ogma_training/` | ML-based symbolic solver (burn) | Learned simplification for the expression engine |
| `data_training/` | Training datasets (JSONL) | Training data for symbolic solver |
| `checkpoints/` | Model checkpoints | Trained solver weights |
| `docs/ml-design.md` | ML architecture docs | Documents solver design |
| `docs/rust-vs-python-ml.md` | ML platform comparison | Documents platform choice |

**Clean up from root `package.json`:**
- Remove scripts: `clean:model`, `clean`, `build:app`, `start`
- Remove devDependencies: `esbuild*`, `chokidar`, `esbuild-server`, `esbuild-plugin-*`, `nodemon`
- Keep scripts: `repl:beam`, `repl` (ml_repl), `check`, `compile`, `watch`, `vscode:install`, `test`, `build:data:*`, `train`, `evaluate`, `rl-train`, `ml:reset`, `lint`

**Clean up from root `Cargo.toml`:**
- Remove workspace members: `erd_model`, `erd_viewer`
- Keep: `ogma_doc`, `ogma_symbolic`, `ogma_training` (all renamed in Phase 1)

### Phase 1: Rename crates and binaries

| Before | After |
|--------|-------|
| `ogma_symbolic/` | `ogma_symbolic/` |
| `ogma_doc/` | `ogma_doc/` |
| `ogma_training/` | `ogma_training/` |
| Cargo workspace member `ogma_symbolic` | `ogma_symbolic` |
| Cargo workspace member `ogma_doc` | `ogma_doc` |
| Cargo workspace member `ogma_training` | `ogma_training` |
| `ogma_doc` depends on `ogma_symbolic` | `ogma_doc` depends on `ogma_symbolic` |
| `ogma_training` depends on `ogma_symbolic` | `ogma_training` depends on `ogma_symbolic` |
| Binary `ogma_check` | `ogma_check` |
| Binary `ogma_compile` | `ogma_compile` |
| Binary `ogma_watch` | `ogma_watch` |
| Binary `ogma_lsp` | `ogma_lsp` |
| Binary `repl` (in ogma_symbolic) | `repl` (in ogma_symbolic) |

**Files to update:**
- `Cargo.toml` (workspace members)
- `ogma_symbolic/Cargo.toml` → `ogma_symbolic/Cargo.toml`
- `ogma_doc/Cargo.toml` → `ogma_doc/Cargo.toml` (name + dependency)
- All `use ogma_symbolic::` → `use ogma_symbolic::` in ogma_doc source
- Binary source files in `ogma_doc/src/bin/` (rename `erd_*.rs` → `ogma_*.rs`)
- `[[bin]]` sections in Cargo.toml if explicit

### Phase 2: Rename file extension

| Before | After |
|--------|-------|
| `.ogma` | `.ogma` |

**Files to update:**
- All `.ogma` test fixture files in `editors/vscode/tests/`
- Test snapshot files (`*.erd.snap` → `*.ogma.snap`)
- Feature docs referencing `.ogma` files
- `package.json` scripts referencing `.ogma`

### Phase 3: Rename VSCode extension

| Before | After |
|--------|-------|
| Extension name: `erd-lang` | `ogma-lang` |
| Language ID: `erd` | `ogma` |
| Publisher: `erd` | `ogma` |
| Display name: `ERD Language Support` | `Ogma Language Support` |
| Config key: `erd.serverPath` | `ogma.serverPath` |
| Scope root: `source.ogma` | `source.ogma` |
| LSP binary: `ogma_lsp` | `ogma_lsp` |
| VSIX file: `erd-lang-0.1.0.vsix` | `ogma-lang-0.1.0.vsix` |

**Files to update:**
- `editors/vscode/package.json` — name, displayName, publisher, license, language ID, file extensions, config keys
- `editors/vscode/src/extension.ts` — config namespace, language ID, client name, binary name
- `editors/vscode/syntaxes/erd.tmLanguage.json` → `ogma.tmLanguage.json` — scope name, all `*.ogma` scopes (133+ occurrences)
- `editors/vscode/snippets/erd.json` → `ogma.json`
- Grammar reference in package.json (path to tmLanguage file)
- Snippet reference in package.json (path to snippet file)

### Phase 4: Update root project files

- `package.json` — name, description, all script references
- `README.md` — rewrite for ogma
- `.gitignore` — update any erd-specific patterns
- `.claude/settings.json` — update permission paths referencing erd

### Phase 5: Rename feature docs

- `features/ogma-syntax.md` → `features/ogma-syntax.md`
- Update all content references from "ERD" to "Ogma" in feature files
- Update `features/README.md` index

### Phase 6: Update tmm repo

- Rename all `.ogma` files to `.ogma` in `src/erd/` → `src/ogma/`
- Update `package.json` scripts to reference ogma binaries
- Update any erd references in build scripts

## Implementation Notes

- Phase 0 was a separate commit so removed code is recoverable from git history
- The `before-ogma` tag marks the last commit before any rename work
- `Cargo.lock` auto-regenerated after crate renames
- After Phase 3, the old VSCode extension was uninstalled before installing the new one
- All phases completed. Crates are `ogma_symbolic`, `ogma_doc`, `ogma_training`. Extension is `ogma-lang`. File extension is `.ogma`.

## Verification

- `cargo test --workspace` passes after each phase
- `npm run vscode:install` succeeds and extension activates on `.ogma` files
- `npm run build` in tmm repo still produces a PDF
- LSP provides diagnostics in VSCode for `.ogma` files
- REPL still works via `npm run repl:beam`
