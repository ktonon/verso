# Rust vs Python for ML Training: Tradeoffs Analysis

## Context

The ERD project currently splits ML work between Rust and Python:

- **Rust**: Expression AST, rule engine, tokenization, validation binary, training data generation
- **Python**: PyTorch transformer model, supervised training, REINFORCE training, evaluation orchestration

This split creates two concrete performance problems observed during M6 (REINFORCE training):

1. **Subprocess overhead**: Each RL training step calls the Rust `validate` binary via `subprocess.run()`, writing/reading JSONL through temp files and stdin/stdout. For batch_size=32, this dominates wall-clock time.
2. **GPU underutilization**: The model is small (1.4M params). MPS (Apple Silicon GPU) was actually 2x *slower* than CPU because the bottleneck is subprocess I/O, not matrix math. The GPU sits idle waiting for validation results.

Additionally, there is a **complexity cost** in maintaining synchronized data contracts (vocab.json, JSONL schemas, token string mappings) between two languages.

## Current Architecture

```
Rust (erd_symbolic)              Python (erd_training)
─────────────────────            ─────────────────────
gen_data binary                  train.py (supervised)
  → vocab.json ──────────────→  vocab.py (reads vocab)
  → *.jsonl ─────────────────→  dataset.py (reads data)

validate binary                  rl_train.py (REINFORCE)
  ← stdin (JSONL) ←─────────── evaluate.py (writes JSONL)
  → stdout (JSONL) ──────────→ evaluate.py (reads results)
```

**What Rust owns**: Expression representation, rule application, correctness checking, data generation.

**What Python owns**: Model architecture, gradient computation, optimizer steps, training loops.

**The bottleneck**: During RL training, every batch requires a round-trip through the validate subprocess. This serializes/deserializes expressions, spawns a process, and blocks the training loop.

## Option A: Pure Rust with Burn

[Burn](https://burn.dev) (v0.20, Jan 2026) is the most mature Rust ML training framework. It has built-in `TransformerEncoder` and `TransformerDecoder` modules, full autograd, AdamW optimizer, and GPU backends via CubeCL (CUDA, Metal, Vulkan, WebGPU).

### What changes

| Component | Current (Python) | Pure Rust (Burn) |
|---|---|---|
| Model definition | PyTorch nn.Module | Burn Module trait |
| Training loop | Python for-loop | Rust for-loop |
| Autograd | PyTorch autograd | burn-autodiff backend |
| Optimizer | torch.optim.AdamW | burn::optim::AdamW |
| Data loading | torch Dataset/DataLoader | Burn Dataset/DataLoader |
| Validation | subprocess → validate binary | Direct function call |
| GPU | PyTorch MPS/CUDA | CubeCL Metal/CUDA |
| Sampling | model.sample() in Python | model.sample() in Rust |
| REINFORCE loss | PyTorch tensors | Burn tensors |

### Advantages

**1. Eliminates the subprocess bottleneck entirely.**
Validation becomes a direct function call to `validate_action_sequence()` — no serialization, no process spawn, no I/O. This is the single biggest win. During RL training, validation is called once per batch (32 examples). Eliminating subprocess overhead could reduce per-batch time by 50-80%.

**2. No data contract synchronization.**
Token types, rule IDs, and expression types are shared Rust types. No vocab.json export step. No risk of Python/Rust schema drift. The model's decoder vocabulary is derived directly from `IndexedRuleSet` at runtime.

**3. Single build system.**
`cargo build` compiles everything. No Python virtualenv, no pip dependencies, no PYTHONPATH gymnastics.

**4. Memory efficiency.**
Rust's ownership model means no GC pauses. Training data can be memory-mapped. Expressions are compact stack-allocated enums, not heap-allocated Python dicts.

**5. Type safety across the full pipeline.**
Rule direction IDs, token indices, and position values are checked at compile time. Currently these are `int` values that must match between languages by convention.

**6. Deployment simplicity.**
The trained model lives in the same binary as the rule engine. No ONNX export step needed for inference (M7 planned milestone).

### Disadvantages

**1. Burn's API is more verbose than PyTorch.**
Burn uses Rust generics extensively. A simple model definition requires explicit backend type parameters:

```rust
// Burn
#[derive(Module, Debug)]
struct MyModel<B: Backend> {
    encoder: TransformerEncoder<B>,
    decoder: TransformerDecoder<B>,
    output: Linear<B>,
}

impl<B: Backend> MyModel<B> {
    fn forward(&self, enc: Tensor<B, 2, Int>, dec: Tensor<B, 2, Int>) -> Tensor<B, 3> {
        // ...
    }
}
```

```python
# PyTorch
class MyModel(nn.Module):
    def __init__(self):
        self.encoder = nn.TransformerEncoder(...)
        self.decoder = nn.TransformerDecoder(...)
        self.output = nn.Linear(...)

    def forward(self, enc, dec):
        # ...
```

**2. Smaller ecosystem and community.**
PyTorch has thousands of tutorials, StackOverflow answers, and pre-built components. Burn has ~14k GitHub stars and a Discord. When you hit a problem, you may need to read Burn's source code rather than finding a blog post.

**3. No built-in RL primitives.**
REINFORCE must be implemented manually. The core math (log_softmax, gather, masked sum) exists as tensor operations, but there are no high-level RL abstractions. This is roughly the same amount of work as what we already wrote in `rl_train.py`.

**4. Iteration speed.**
Rust compile times (especially with GPU backends) are slower than Python's edit-run cycle. Hyperparameter tuning involves more recompilation. Mitigated somewhat by `cargo watch` and incremental compilation, but still slower than `python train.py --lr 1e-4`.

**5. Risk of framework immaturity.**
Burn is actively developed but still pre-1.0. APIs may change between minor versions. Edge cases in the transformer implementation or Metal backend could require workarounds. PyTorch is battle-tested at massive scale.

**6. Checkpoint portability.**
Burn checkpoints are Rust-specific (MessagePack or similar). If you ever need to use the model from Python (e.g., for analysis, visualization, or integration with other tools), you'd need an export step. PyTorch checkpoints are widely interoperable.

### Effort estimate

- Rewrite model.py → Burn Module: **Medium** (Burn has TransformerEncoder/Decoder built-in)
- Rewrite train.py → Burn training loop: **Medium** (standard supervised loop)
- Rewrite rl_train.py → Burn REINFORCE: **Medium** (tensor ops are available, RL logic is custom)
- Rewrite dataset.py → Burn DataLoader: **Low** (JSONL reading is straightforward)
- Eliminate vocab.py: **Free** (use Rust types directly)
- Eliminate evaluate.py subprocess: **Free** (call validate_action_sequence directly)

Total: **~1-2 weeks** of focused work, assuming no major Burn roadblocks.

## Option B: Keep Python, Eliminate Subprocess via PyO3

Use [PyO3](https://pyo3.rs) to compile the Rust validation logic as a native Python extension module. Python calls Rust directly through FFI — no subprocess, no serialization.

### What changes

| Component | Current | With PyO3 |
|---|---|---|
| Validation | subprocess.run() | `erd_native.validate_batch(entries)` |
| Everything else | Same | Same |

### Advantages

**1. Eliminates subprocess bottleneck** without rewriting the ML code.

**2. Minimal disruption.** The Python training code stays as-is. Only `run_validation_batch()` in evaluate.py changes from a subprocess call to a function call.

**3. Preserves PyTorch ecosystem.** Keep all the mature tooling, tutorials, and community support.

**4. Incremental adoption.** Can expose more Rust functions over time (tokenization, expression parsing) without committing to a full rewrite.

### Disadvantages

**1. Adds build complexity.** PyO3 requires `maturin` or `setuptools-rust` for building the native extension. The build process becomes: `cargo build` + `maturin develop` + `python train.py`.

**2. Data marshaling at the FFI boundary.** Expressions, tokens, and actions still need to be converted between Python dicts and Rust structs. PyO3 handles this, but it's not free — there's serialization overhead (though much less than subprocess I/O).

**3. Doesn't solve the data contract problem.** vocab.json and JSONL schemas still need to be synchronized. Token IDs still need to match by convention.

**4. Two-language complexity remains.** Debugging spans Rust and Python. Stack traces cross the FFI boundary. New developers need both toolchains.

### Effort estimate

- Create PyO3 crate wrapping validate logic: **Low-Medium** (~2-3 days)
- Update evaluate.py to use native module: **Low** (~1 day)
- Build system integration (maturin): **Low** (~1 day)

Total: **~1 week**

## Option C: Keep Current Architecture (Status Quo)

### Advantages

- Already working. M6 completed successfully.
- PyTorch is the industry standard. Any ML practitioner can understand the code.
- The subprocess overhead, while suboptimal, is bounded. With batch_size=32 and ~30k training examples, each epoch has ~940 subprocess calls. At ~0.5s each, that's ~8 minutes of validation overhead per epoch.

### Disadvantages

- Subprocess bottleneck limits RL training throughput.
- GPU underutilization (model too small, bottleneck is I/O).
- Two-language maintenance burden persists.
- M7 (ONNX inference) requires yet another format translation layer.

## Option D: Candle (Alternative Rust Framework)

[Candle](https://github.com/huggingface/candle) by Hugging Face (19k stars) is another pure-Rust ML framework. Its API is more PyTorch-like than Burn's, with less generics boilerplate. However:

- **No built-in TransformerEncoder/Decoder** — you'd implement these yourself
- **Inference-focused** — training support exists but is secondary
- **Metal support** via metal-candle crate
- Reported **3-4x slower GPU performance** than PyTorch for some workloads

Candle would be a reasonable choice if Burn's verbosity is a dealbreaker, but the lack of built-in transformer modules adds work. For this project, Burn is the stronger fit.

## Comparison Matrix

| Factor | A: Pure Rust (Burn) | B: PyO3 Bridge | C: Status Quo | D: Candle |
|---|---|---|---|---|
| Subprocess eliminated | Yes | Yes | No | Yes |
| Vocab sync eliminated | Yes | No | No | Yes |
| GPU utilization | Good (CubeCL) | Same as now | Poor | Good |
| PyTorch ecosystem | Lost | Kept | Kept | Lost |
| Build complexity | Simple (cargo) | Medium (maturin) | Simple | Simple (cargo) |
| Iteration speed | Slower (compile) | Same as now | Fast | Slower (compile) |
| Framework maturity | Pre-1.0 | N/A | Battle-tested | Pre-1.0 |
| M7 (inference) | Free (same binary) | Still needs ONNX | Needs ONNX | Free (same binary) |
| Effort | ~1-2 weeks | ~1 week | 0 | ~2-3 weeks |
| Risk | Medium (Burn edge cases) | Low | None | Medium-High |

## Recommendation

**Option A (Pure Rust with Burn)** is the strongest long-term choice for this project specifically, for these reasons:

1. **The model is small** (1.4M params, 4 encoder + 4 decoder layers, d_model=128). We're not training GPT — we're training a compact policy network. Burn's transformer modules are well-suited for this scale. The risk of hitting framework limitations at large scale doesn't apply.

2. **The validation bottleneck is the primary pain point**, and only Options A/B/D eliminate it. But Option A also eliminates the data contract synchronization, the ONNX export step (M7), and the two-language maintenance burden.

3. **The project is already Rust-first.** The rule engine, parser, tokenizer, expression representation, and data generation are all Rust. Python is only used for the ~500 lines of ML code. Making that Rust too creates a single coherent codebase.

4. **M7 becomes trivial.** The current plan calls for ONNX export for Rust inference. With a Burn model, inference is just `model.forward()` in the same binary — no export, no runtime dependency.

**However**, if rapid iteration on training hyperparameters and RL algorithms is the near-term priority, **Option B (PyO3)** gives the best effort-to-impact ratio. It solves the immediate subprocess bottleneck in ~1 week while preserving the PyTorch ecosystem for experimentation.

A pragmatic path: **Start with Option B now** to unblock faster RL training, then **migrate to Option A** when the training pipeline stabilizes and you're ready for M7.
