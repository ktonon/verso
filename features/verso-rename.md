# Verso Rename

## Goal

Rebrand the project from **erd** (Emergent Rung Dynamics) to **verso** (Verifiable Source). The project has evolved from a physics simulation into a paper-writing tool with machine-verified math. The new name reflects its actual purpose: treating scientific papers as verifiable source code.

This rename also cleans up components that no longer belong in a paper-writing tool.

## Plan

### Phase 0: Remove non-paper components

These components predate the paper-writing pivot and are no longer part of verso's mission.

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
| `erd_training/` | ML-based symbolic solver (burn) | Learned simplification for the expression engine |
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
- Keep: `erd_doc`, `erd_symbolic`, `erd_training` (all renamed in Phase 1)

### Phase 1: Rename crates and binaries

| Before | After |
|--------|-------|
| `erd_symbolic/` | `verso_symbolic/` |
| `erd_doc/` | `verso_doc/` |
| `erd_training/` | `verso_training/` |
| Cargo workspace member `erd_symbolic` | `verso_symbolic` |
| Cargo workspace member `erd_doc` | `verso_doc` |
| Cargo workspace member `erd_training` | `verso_training` |
| `erd_doc` depends on `erd_symbolic` | `verso_doc` depends on `verso_symbolic` |
| `erd_training` depends on `erd_symbolic` | `verso_training` depends on `verso_symbolic` |
| Binary `erd_check` | `verso_check` |
| Binary `erd_compile` | `verso_compile` |
| Binary `erd_watch` | `verso_watch` |
| Binary `erd_lsp` | `verso_lsp` |
| Binary `repl` (in erd_symbolic) | `repl` (in verso_symbolic) |

**Files to update:**
- `Cargo.toml` (workspace members)
- `erd_symbolic/Cargo.toml` → `verso_symbolic/Cargo.toml`
- `erd_doc/Cargo.toml` → `verso_doc/Cargo.toml` (name + dependency)
- All `use erd_symbolic::` → `use verso_symbolic::` in erd_doc source
- Binary source files in `erd_doc/src/bin/` (rename `erd_*.rs` → `verso_*.rs`)
- `[[bin]]` sections in Cargo.toml if explicit

### Phase 2: Rename file extension

| Before | After |
|--------|-------|
| `.erd` | `.verso` |

**Files to update:**
- All `.erd` test fixture files in `editors/vscode/tests/`
- Test snapshot files (`*.erd.snap` → `*.verso.snap`)
- Feature docs referencing `.erd` files
- `package.json` scripts referencing `.erd`

### Phase 3: Rename VSCode extension

| Before | After |
|--------|-------|
| Extension name: `erd-lang` | `verso-lang` |
| Language ID: `erd` | `verso` |
| Publisher: `erd` | `verso` |
| Display name: `ERD Language Support` | `Verso Language Support` |
| Config key: `erd.serverPath` | `verso.serverPath` |
| Scope root: `source.erd` | `source.verso` |
| LSP binary: `erd_lsp` | `verso_lsp` |
| VSIX file: `erd-lang-0.1.0.vsix` | `verso-lang-0.1.0.vsix` |

**Files to update:**
- `editors/vscode/package.json` — name, displayName, publisher, license, language ID, file extensions, config keys
- `editors/vscode/src/extension.ts` — config namespace, language ID, client name, binary name
- `editors/vscode/syntaxes/erd.tmLanguage.json` → `verso.tmLanguage.json` — scope name, all `*.erd` scopes (133+ occurrences)
- `editors/vscode/snippets/erd.json` → `verso.json`
- Grammar reference in package.json (path to tmLanguage file)
- Snippet reference in package.json (path to snippet file)

### Phase 4: Update root project files

- `package.json` — name, description, all script references
- `README.md` — rewrite for verso
- `.gitignore` — update any erd-specific patterns
- `.claude/settings.json` — update permission paths referencing erd

### Phase 5: Rename feature docs

- `features/erd-syntax.md` → `features/verso-syntax.md`
- Update all content references from "ERD" to "Verso" in feature files
- Update `features/README.md` index

### Phase 6: Update tmm repo

- Rename all `.erd` files to `.verso` in `src/erd/` → `src/verso/`
- Update `package.json` scripts to reference verso binaries
- Update any erd references in build scripts

## Implementation Notes

- Phase 0 should be a separate commit so removed code is recoverable from git history
- The `before-verso` tag marks the last commit before any rename work
- `Cargo.lock` will auto-regenerate after crate renames
- After Phase 3, the old VSCode extension should be uninstalled before installing the new one
- `erd_symbolic` is the core algebra engine — depended on by both `verso_doc` and `verso_training`, so it becomes `verso_symbolic`
- `erd_training` provides the learned symbolic solver — it stays as `verso_training`

## Verification

- `cargo test --workspace` passes after each phase
- `npm run vscode:install` succeeds and extension activates on `.verso` files
- `npm run build` in tmm repo still produces a PDF
- LSP provides diagnostics in VSCode for `.verso` files
- REPL still works via `npm run repl:beam`
