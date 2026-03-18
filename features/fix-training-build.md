# Fix Training Build

## Goal

The `verso_training` crate fails to compile due to a `wgpu_core` type overflow error. This blocks all ML training pipeline work (supervised training, RL fine-tuning, evaluation).

## Plan

1. **Diagnose the root cause** — The error is `overflow evaluating the requirement wgpu_core::validation::NumericDimension: Sync`. This is a known issue with certain versions of `burn` + `wgpu-core`. The current pinned version is `burn = "0.20"`.
2. **Upgrade burn** — Check for a newer burn version that resolves the wgpu compatibility issue. Burn releases frequently and wgpu integration is actively maintained.
3. **Adapt API changes** — Burn major versions often change APIs (model definition, training loop, backend initialization). Audit all `verso_training/src/` files for breaking changes.
4. **Verify build and tests** — `cargo build --package verso_training` and `cargo test --package verso_training` must pass.
5. **Validate training pipeline** — Run `npm run build:data`, `npm run train`, `npm run evaluate` to confirm end-to-end functionality.

### Key files
- `verso_training/Cargo.toml` — burn version pin
- `verso_training/src/model.rs` — Transformer encoder-decoder (Burn Module)
- `verso_training/src/train.rs` — Supervised training loop
- `verso_training/src/rl_train.rs` — REINFORCE training loop
- `verso_training/src/evaluate.rs` — Model evaluation
- `verso_training/src/dataset.rs` — JSONL data loading + Burn Batcher
- `verso_training/src/config.rs` — CLI configs
- `verso_training/src/schedule.rs` — Cosine LR schedule

### Error details
```
error[E0275]: overflow evaluating the requirement `wgpu_core::validation::NumericDimension: Sync`
```
Triggered by `burn 0.20` pulling `wgpu-core 26.0.1`. The overflow is in the Sync trait bound evaluation chain through deeply nested wgpu types.

## Implementation Notes

*Not started yet.*

## Verification

- [ ] `cargo build --package verso_training` compiles without errors
- [ ] `cargo test --package verso_training` passes
- [ ] `npm run build:data` generates training data
- [ ] `npm run train` runs at least one epoch successfully
- [ ] `npm run evaluate` produces evaluation output
