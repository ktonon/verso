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

**1. Risk of framework immaturity.**
Burn is actively developed but still pre-1.0. APIs may change between minor versions. Edge cases in the transformer implementation or Metal backend could require workarounds that aren't documented. If we hit a Burn bug, debugging requires reading framework internals. PyTorch is battle-tested at massive scale.

**2. Compile times affect the feedback loop.**
Rust compile times (especially with GPU backends) are slower than Python's instant edit-run cycle. Incremental compilation helps, but a clean build with CubeCL could take minutes. Hyperparameters should be exposed as CLI args to avoid recompilation for tuning.

**3. Checkpoint portability.**
Burn checkpoints are Rust-specific. If we ever need to inspect the model from Python (analysis, visualization), we'd need an export step.

### Non-issues (LLM-assisted development)

The following are commonly cited disadvantages of Burn vs PyTorch that **do not apply** when an LLM is writing the code:

- **API verbosity**: Burn's generics-heavy API requires more boilerplate than PyTorch. An LLM generates verbose Rust as easily as concise Python — typing speed is not a bottleneck.
- **Smaller ecosystem**: PyTorch has thousands of tutorials and StackOverflow answers. An LLM has knowledge of Burn's API and can read source code directly when documentation is sparse.
- **No built-in RL primitives**: REINFORCE must be implemented manually with tensor ops. We already wrote this logic once in Python; translating it to Burn tensor ops is mechanical.
- **Two-framework learning curve**: There is no second developer who needs to learn Burn. The LLM already knows both frameworks.

### Effort estimate

An LLM writes the Burn model, training loops, and REINFORCE logic directly from the existing Python implementations. The translation is largely mechanical — same algorithms, different tensor API.

- Rewrite model.py → Burn Module: **Low** (Burn has TransformerEncoder/Decoder built-in, direct translation)
- Rewrite train.py → Burn training loop: **Low** (standard supervised loop, same structure)
- Rewrite rl_train.py → Burn REINFORCE: **Low-Medium** (tensor ops exist, RL logic translates directly)
- Rewrite dataset.py → Burn DataLoader: **Low** (JSONL reading is straightforward)
- Eliminate vocab.py: **Free** (use Rust types directly)
- Eliminate evaluate.py subprocess: **Free** (call validate_action_sequence directly)

Total: **~1-3 sessions** of LLM-assisted work, assuming no major Burn roadblocks. The primary risk is Burn edge cases, not implementation effort.

## Option B: Keep Python, Eliminate Subprocess via PyO3

Use [PyO3](https://pyo3.rs) to compile the Rust validation logic as a native Python extension module. Python calls Rust directly through FFI — no subprocess, no serialization.

### What changes

| Component | Current | With PyO3 |
|---|---|---|
| Validation | subprocess.run() | `erd_native.validate_batch(entries)` |
| Everything else | Same | Same |

### Advantages

**1. Eliminates subprocess bottleneck** without rewriting the ML code.

**2. Preserves PyTorch ecosystem** for experimentation.

### Disadvantages

**1. Adds build complexity.** PyO3 requires `maturin` or `setuptools-rust`. The build process becomes: `cargo build` + `maturin develop` + `python train.py`. A third build tool in an already two-language project.

**2. Data marshaling at the FFI boundary.** Expressions, tokens, and actions still need to be converted between Python dicts and Rust structs. PyO3 handles this, but there's serialization overhead (though much less than subprocess I/O).

**3. Doesn't solve the data contract problem.** vocab.json and JSONL schemas still need to be synchronized. Token IDs still need to match by convention.

**4. Two-language complexity remains** — and now with a third build tool (maturin).

**5. Solves only the immediate bottleneck.** The fundamental cost of maintaining two languages, syncing data contracts, and needing ONNX for M7 all remain. This is a band-aid.

### Effort estimate

Total: **~1 session** of LLM-assisted work. But every session spent on PyO3 plumbing is a session not spent on Option A, which solves all the same problems plus more.

## Option C: Keep Current Architecture (Status Quo)

### Advantages

- Already working. M6 completed successfully.

### Disadvantages

- Subprocess bottleneck limits RL training throughput.
- GPU underutilization (model too small, bottleneck is I/O).
- Two-language maintenance burden persists.
- M7 (ONNX inference) requires yet another format translation layer.
- "PyTorch is the industry standard" matters for team onboarding. With LLM-assisted development, the framework choice matters less than the architecture quality.

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
| Build complexity | Simple (cargo) | Worse (maturin) | Two languages | Simple (cargo) |
| Framework maturity | Pre-1.0 | N/A | Battle-tested | Pre-1.0 |
| M7 (inference) | Free (same binary) | Still needs ONNX | Needs ONNX | Free (same binary) |
| Languages in project | 1 | 3 (Rust + Python + FFI) | 2 | 1 |
| Effort (LLM-assisted) | ~1-3 sessions | ~1 session | 0 | ~2-4 sessions |
| Risk | Medium (Burn edge cases) | Low | None | Medium-High |

## Recommendation

**Option A (Pure Rust with Burn).** The reasoning:

1. **The model is small** (1.4M params, 4 encoder + 4 decoder layers, d_model=128). We're not training GPT — we're training a compact policy network. Burn's transformer modules are well-suited for this scale.

2. **The validation bottleneck is the primary pain point**, and Option A eliminates it along with the data contract synchronization, the ONNX export step (M7), and the two-language maintenance burden. Option B only fixes the first.

3. **The project is already Rust-first.** The rule engine, parser, tokenizer, expression representation, and data generation are all Rust. Python is only used for ~500 lines of ML code. Making that Rust too creates a single coherent codebase.

4. **M7 becomes trivial.** The current plan calls for ONNX export for Rust inference. With a Burn model, inference is just `model.forward()` in the same binary — no export, no runtime dependency.

5. **LLM-assisted development changes the calculus.** The traditional argument for Option B ("do the quick fix now, big rewrite later") assumes the rewrite is expensive. With an LLM writing the code, the effort difference between Option A and B is ~1-2 sessions, not weeks. The incremental approach adds complexity (maturin, FFI layer) that we'd throw away during the inevitable migration to Option A.

Option B is a local optimum: less risk, less reward, and leaves technical debt. Option A is the global optimum: slightly more risk from Burn edge cases, but eliminates an entire language from the project and solves M7 for free.

**Skip the band-aid. Go straight to Burn.**
