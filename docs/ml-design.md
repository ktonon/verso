# ML-Guided Simplification: Design Document

## Motivation

The current beam search explores rewrites broadly but blindly. A trace of `x*x + y*y + 2x*y` simplifying to `(y + x)**2` takes 20 steps, mostly spent shuffling terms with `add_commute`, `mul_commute`, and `add_assoc` until the pattern matcher can see the perfect square. A learned policy should reach the same result in 3-5 steps by recognizing the structure directly.

```
Current beam search (20 steps):
0: x*x + y*y + 2x*y
1: 2x*y + x*x + y*y       add_commute
2: 2x*y + x*x + y*y       add_assoc
   ... 17 more commute/assoc/distribute steps ...
20: (y + x)**2             perfect_square_plus_1

Goal (3 steps):
0: x*x + y*y + 2x*y
1: x**2 + y**2 + 2x*y     mul_self_square (×2)
2: x**2 + 2x*y + y**2     add_commute
3: (x + y)**2              perfect_square_plus_1
```

## Architecture Overview

The model is a **sequence-to-sequence** transformer. Given a tokenized expression, it outputs a complete simplification plan — a sequence of `(rule_id, position)` actions. The plan is validated by applying the rules to the AST in order.

```
                    ┌─────────────────────┐
                    │  Expression (AST)   │
                    └────────┬────────────┘
                             │ tokenize
                             ▼
                    ┌─────────────────────┐
                    │  Token Sequence     │
                    │  [ADD, MUL, V0, V0, │
                    │   ADD, MUL, V1, V1, │
                    │   MUL, MUL, 2, V0,  │
                    │   V1]               │
                    └────────┬────────────┘
                             │
                             ▼
                    ┌─────────────────────┐
                    │  Transformer        │
                    │  Encoder            │
                    └────────┬────────────┘
                             │
                             ▼
                    ┌─────────────────────┐
                    │  Transformer        │
                    │  Decoder            │
                    └────────┬────────────┘
                             │
                             ▼
                    ┌─────────────────────┐
                    │  Action Sequence    │
                    │  [(rule, pos),      │
                    │   (rule, pos),      │
                    │   (rule, pos),      │
                    │   STOP]             │
                    └────────┬────────────┘
                             │
                             ▼
                    ┌─────────────────────┐
                    │  Validate & Apply   │
                    │  (Rust rule engine) │
                    └────────┬────────────┘
                             │
                             ▼
                    ┌─────────────────────┐
                    │  Simplified Expr    │
                    └─────────────────────┘
```

The model predicts the **entire plan from the initial expression**. It must learn to mentally simulate how each rule transforms the AST in order to predict that later rules will be valid. This is the core capability that makes it useful.

## 1. Expression Tokenization

Serialize the AST via pre-order traversal. Each AST node becomes one token; each token has a position index used as the action target.

### Token Vocabulary

| Category | Tokens | Notes |
|----------|--------|-------|
| Structure | `ADD`, `MUL`, `NEG`, `INV`, `POW` | Binary/unary ops |
| Functions | `SIN`, `COS`, `TAN`, `EXP`, `LN`, ... | One per `FnKind` variant (18 total) |
| Variables | `V0`, `V1`, `V2`, ... | De Bruijn-style: first unique var = V0, second = V1, etc. |
| Integers | `I_-2`, `I_-1`, `I_0`, `I_1`, `I_2`, ..., `I_12` | Small integer tokens for `Rational(n/1)` |
| Fractions | `FRAC`, num_tok, den_tok | For non-integer `Rational` |
| Pi multiples | `FRAC_PI`, num_tok, den_tok | For `FracPi(r)` |
| Named | `E`, `SQRT2`, `SQRT3`, `INV_SQRT2`, `SQRT3_2` | One per `NamedConst` variant |
| Decimal | `CONST`, mantissa tokens | For `Const(f64)`, rare in practice |

### De Bruijn Variable Naming

Variable identity matters (x vs y) but specific names don't. Canonicalize by order of first appearance in pre-order traversal:

```
x*x + y*y + 2*x*y  →  V0*V0 + V1*V1 + 2*V0*V1
a*b + b*a           →  V0*V1 + V1*V0
```

This lets the model generalize across variable names. At inference time, maintain a mapping back to original names.

### Example

```
Expression: x*x + y*y + 2*x*y

AST: Add(Mul(Var(x), Var(x)), Add(Mul(Var(y), Var(y)), Mul(Mul(Rat(2), Var(x)), Var(y))))

Tokens:  [ADD, MUL, V0, V0, ADD, MUL, V1, V1, MUL, MUL, I_2, V0, V1]
Index:    0    1    2   3   4    5    6   7   8    9    10  11  12
```

Position index 5 = the `MUL` node for `y*y`. Applying `mul_self_square` at position 5 rewrites `y*y` to `y**2`.

### Tensor Indices

Variables with tensor indices get additional tokens:

```
g_(mu,nu)  →  [VAR, V0, IDX_LO, IX0, IDX_LO, IX1]
```

Index names also use De Bruijn-style canonicalization.

## 2. Action Space

Each action in the output sequence is a tuple: `(rule_id, direction, position)`.

### Rule Indexing

Assign each rule a stable integer ID (0..N-1). Current count: 195 rules. Non-reversible rules have one direction; reversible rules have two. Flatten into a single index space:

```rust
struct Action {
    rule_direction: u16,   // 0..252 (195 LTR + 58 RTL)
    position: u16,         // node index in pre-order token sequence
}
```

Total rule-direction pairs: ~253 (195 base + 58 reversible RTL).

The output vocabulary also includes a `STOP` token to end the sequence.

### Output Sequence Format

The decoder outputs a sequence of action tokens:

```
[RULE_42, POS_5, RULE_17, POS_4, RULE_93, POS_0, STOP]
 ╰─ action 1 ─╯  ╰─ action 2 ─╯  ╰─ action 3 ─╯
```

Each action is two tokens (rule, position), terminated by STOP.

### Position Encoding Across Transformations

Position indices refer to the **initial** expression's token positions. As rules transform the AST, positions shift. The model must learn this mapping implicitly — predicting that position 5 in the original expression corresponds to a particular subtree even after earlier rules have changed the structure around it.

For the initial implementation, positions refer to the original AST. A future refinement could re-index after each step, but this adds complexity to the output vocabulary (positions become step-dependent).

## 3. Training Data Generation

### Random Expression Generation

Generate expressions by recursive random construction:

```
gen_expr(depth, max_depth) →
    if depth == max_depth:
        random leaf (Var, Rational, FracPi, Named)
    else:
        random node type (Add, Mul, Pow, Neg, Fn, ...)
        with children = gen_expr(depth+1, max_depth)
```

Control distribution over node types to match realistic expressions:
- Higher weight on Add, Mul (common)
- Lower weight on Pow, trig functions (less common)
- Depth 3-6 for moderate complexity

### Beam Search Bootstrap Data

Beam search provides initial training data — valid traces, not optimal ones.

For each generated expression:

1. Run **randomized** beam search N times (N=5-10) with shuffled rule order and stochastic beam selection
2. Record each derivation path: sequence of `(rule_id, direction, position)`
3. Discard examples where beam search makes no progress (already simplified)
4. Keep the shortest valid trace per expression as the training target

Randomization is critical: a deterministic beam search produces one trace per input, usually a bad one. Among N random traces, the shortest one teaches the model to be as good as the *luckiest* beam search run.

### Training Example Format

Each example is one complete simplification:

```json
{
    "input_tokens": [ADD, MUL, V0, V0, ADD, MUL, V1, V1, MUL, MUL, I_2, V0, V1],
    "action_sequence": [
        { "rule_direction": 42, "position": 1 },
        { "rule_direction": 42, "position": 5 },
        { "rule_direction": 17, "position": 4 },
        { "rule_direction": 93, "position": 0 }
    ],
    "output_complexity": 5,
    "input_complexity": 13
}
```

### Position Annotation

Each trace step records the AST path where the rule was applied. During export, the path is converted to a pre-order token index via `path_to_position()`.

### Data Volume & Curriculum Rounds

Training data is generated in three rounds with increasing complexity, using different seeds to avoid overlap:

| Round | Complexity | Seed | Purpose |
|-------|-----------|------|---------|
| **Round 1** | 3–12 | 42 | Learn individual rules: short token sequences (3-15 tokens), mostly 1-3 step traces |
| **Round 2** | 8–20 | 43 | Learn multi-step chains: overlapping range provides continuity, 2-5 step traces |
| **Round 3** | 3–unlimited | 44 | Full distribution: generalize to longer traces and higher complexity |

Each round generates 10K expressions with 5 beam search runs each (`npm run build:data`). Higher `--min-complexity` reduces the skip rate by filtering out trivially simple expressions before running beam search. Different seeds per round ensure distinct expression populations despite the overlapping complexity ranges.

## 4. Model Architecture

### Encoder

A small transformer encoder (4-6 layers, 256-dim embeddings, 4 heads). Input is the tokenized expression with:

- **Token embedding**: learned embedding per vocabulary item
- **Position embedding**: learned per-position (up to max sequence length ~128)
- **Depth embedding**: tree depth of each token in the AST (helps the model understand structure)

### Decoder

A transformer decoder that generates the action sequence autoregressively. At each step it attends to:

1. The encoder output (cross-attention over expression tokens)
2. Previously generated actions (causal self-attention)

Output vocabulary: ~253 rule-direction tokens + ~128 position tokens + STOP.

```
decoder_input:   [BOS]
decoder_output:  [RULE_42, POS_1, RULE_42, POS_5, RULE_17, POS_4, RULE_93, POS_0, STOP]
```

The decoder generates rule and position tokens alternately. This is natural for a transformer — it learns the alternating pattern from data.

### Model Size

Target ~1-2M parameters. This is a small model by modern standards, but the task is narrow and well-structured. The encoder needs to understand expression structure; the decoder needs to output short sequences (typically <20 actions = <40 tokens).

## 5. Training Pipeline

### Key principle: beam search is not the oracle

Beam search provides *valid* transformations, not *optimal* ones. It serves two roles:

1. **Bootstrap data source** — generate initial traces so the model has something to learn from
2. **Complexity baseline** — "beam search achieved complexity X; can you do better?"

Beam search is never treated as the authority on which action is best. The model should eventually surpass beam search by learning what makes a good *sequence* of transformations.

### Key principle: reward the endpoint, not intermediate steps

Some simplification paths require temporarily **increasing** complexity before reaching a simpler form. For example, `distribute_left` expands a product (complexity goes up) to enable `perfect_square_plus_1` later (complexity comes way down). Any per-step complexity penalty would train the model to avoid these necessary expansions, trapping it in local minima.

The reward is based solely on the **final result** of applying the full action sequence: was it valid? Did complexity decrease? By how much relative to the number of steps?

### Sequence validation

Given an input expression and a predicted action sequence:

1. Apply each `(rule, position)` to the AST in order
2. If a rule fails to pattern-match at its position → the sequence is **invalid** from that point onward
3. The final expression is whatever state the AST is in after the last valid action (or the original if the first action fails)

```
Score a sequence:
  valid_steps = number of actions that successfully apply
  total_steps = length of predicted sequence
  invalid_steps = total_steps - valid_steps

  if invalid_steps > 0:
      penalty = -invalid_steps * INVALID_PENALTY
  complexity_delta = input_complexity - output_complexity
  reward = complexity_delta / max(valid_steps, 1) + penalty
```

Invalid sequences receive heavy penalties. This is the primary learning signal early in training — the model must first learn which rules are structurally valid before it can optimize for efficiency.

### Phase 1: Supervised warm start

Train on beam search traces using standard seq2seq cross-entropy loss.

- **Data**: shortest trace per expression from randomized beam search runs
- **Loss**: cross-entropy on the action sequence (teacher forcing)
- **Optimizer**: AdamW, cosine LR schedule
- **Epochs**: until validation loss plateaus

The model learns the structure of valid action sequences — which rules tend to apply to which expression shapes, and what reasonable simplification plans look like. It doesn't learn that beam search's specific choices are optimal.

### Phase 2: Self-play with trajectory reward

The model generates action sequences on its own and is rewarded based on outcomes.

1. Sample expressions (mix of training set and fresh random ones)
2. Run the model to produce an action sequence
3. Validate the sequence by applying rules to the AST
4. Score: reward valid sequences that reduce complexity, penalize invalid ones
5. Use REINFORCE or PPO to update the model weights

No beam search involved. The model explores freely. Key behaviors it learns:

- **Validity**: stop predicting rules that can't match (penalty signal)
- **Efficiency**: shorter sequences that achieve the same reduction score higher
- **Planning**: sometimes increasing complexity mid-sequence enables a bigger reduction later (no per-step penalty prevents this)
- **Surpassing beam search**: if the model finds a lower-complexity result than beam search's baseline for an expression, that becomes the new baseline

### Phase 3: Curriculum

Start with simple expressions and gradually increase complexity. This builds composable skills:

1. **Single-rule** (depth 2): `x + 0`, `x * 1`, `sin(0)`
2. **Two-rule chains** (depth 3): `x + 0 + y`, `sin(0) + cos(0)`
3. **Multi-rule with commutativity** (depth 3-4): `y + x + 0`
4. **Expansion then simplification** (depth 4-5): paths that require complexity to increase before decreasing
5. **Factoring and trig identities** (depth 4-6): `x**2 + 2*x*y + y**2`

The curriculum is applied to both Phase 1 and Phase 2: train supervised on simple expressions first, then self-play on simple expressions, then gradually mix in harder ones.

## 6. Inference Integration

### Rust Runtime

Since training and inference use the same Burn model in Rust, no export step (ONNX, etc.) is needed. The trained model checkpoint is loaded directly:

```rust
fn ml_simplify(expr: &Expr, model: &SimplificationModel<B>, rules: &IndexedRuleSet,
               enc_vocab: &EncoderVocab, dec_vocab: &DecoderVocab) -> (Expr, Vec<TraceStep>) {
    let tokens = tokenize(expr);
    let enc_ids = encode(&tokens, enc_vocab);
    let action_ids = model.generate(enc_ids, max_len, None, STOP_TOKEN);
    let actions = decode_action_sequence(&action_ids, dec_vocab);

    let result = validate_action_sequence(expr, &actions, rules);
    // ... build trace from result
}
```

### Hybrid Strategy

For production use, run the ML model first (fast, usually correct), then fall back to beam search if the ML result has higher complexity than the input:

```rust
fn simplify(expr: &Expr, model: &SimplificationModel<B>, rules: &RuleSet) -> Expr {
    let (ml_result, _trace) = ml_simplify(expr, model, &rules);
    if ml_result.complexity() <= expr.complexity() {
        return ml_result;
    }
    // Fallback: beam search
    beam_search_simplify(expr, rules)
}
```

## 7. Evaluation Metrics

| Metric | Definition | Target |
|--------|-----------|--------|
| **Validity rate** | Fraction of predicted sequences that are fully valid | >95% after Phase 1 |
| **Correctness** | Does the model reach the same simplified form as beam search (or better)? | >99% |
| **Step efficiency** | Ratio of model steps to beam search steps | <0.5 (2x fewer steps) |
| **Latency** | Wall-clock time per simplification | <10ms for depth-4 expressions |
| **Coverage** | Fraction of test expressions where model finds a simplification | >95% |
| **Surpass rate** | Fraction of expressions where model finds a *simpler* result than beam search | Measured, any >0% is a win |

### Test Suites

1. **Unit**: known simplifications from existing tests (398 cases)
2. **Random**: fresh random expressions not in training data
3. **Adversarial**: expressions designed to require long rewrite chains
4. **Expansion-required**: expressions that require increasing complexity before simplifying
5. **Real-world**: expressions from physics/engineering textbooks

## 8. Infrastructure Requirements

### Training

- Rust + Burn framework for model development and training
- Custom data loader that reads trace files (JSONL)
- Single GPU sufficient for the model size (~1-2M parameters), CPU (ndarray) also viable
- Training time: hours, not days

### Data Pipeline

```
┌────────────┐     ┌──────────────────┐     ┌─────────────┐     ┌───────────────┐
│ Random AST │────▶│ Randomized Beam  │────▶│ Trace Export │────▶│ Training      │
│ Generator  │     │ Search (×N runs) │     │ (JSONL)      │     │ (Rust + Burn) │
└────────────┘     └──────────────────┘     └─────────────┘     └───────┬───────┘
                                                                        │
                                                                  ┌─────▼─────┐
                                                                  │ Rust      │
                                                                  │ Inference │
                                                                  └───────────┘
```

### Rust Components

**verso_symbolic** (rule engine):
1. **Tokenizer**: `Expr → Vec<Token>` and reverse, with De Bruijn variable canonicalization
2. **Position mapper**: pre-order index ↔ AST path, `subexpr_at()` and `replace_subexpr()`
3. **Random expression generator**: for training data, with configurable depth/type distributions
4. **Randomized beam search**: shuffled rule order, stochastic beam selection, multi-run
5. **Trace exporter**: serialize traces with position annotations to JSONL

**verso_training** (ML pipeline):
1. **Data loader**: read JSONL files, create Burn Dataset + Batcher with padding
2. **Model definition**: transformer encoder-decoder (Burn Module)
3. **Supervised training loop**: cross-entropy with cosine LR schedule
4. **REINFORCE training loop**: policy gradient with direct validation calls
5. **Evaluation**: metrics computation via direct `validate_action_sequence()` calls

## 9. M4 Results & Observations

### Phase 1 Training Run (100 epochs, CPU)

| Metric | Value |
|--------|-------|
| Model parameters | 1,430,549 |
| Architecture | d_model=128, d_ff=256, 4+4 layers, 4 heads |
| Training examples | ~13.2K (90% of ~14.7K) |
| Validation examples | ~1.5K |
| Final train loss | 0.20 |
| Final val loss | 0.53 |
| Best val loss | ~0.53 (epoch ~85) |
| Time per epoch | ~39s (CPU) |

### Overfitting Analysis

The train/val gap (0.20 vs 0.53) is expected given the data/parameter ratio (~10 examples per parameter). The model memorized training traces but still generalized meaningfully — random guessing would yield loss ~5.6 (ln(277 decoder tokens)).

This is acceptable for Phase 1's purpose: learning the structure of valid action sequences as a warm start for Phase 2 (self-play/RL). The key metric is M5's validity rate — what fraction of the model's predicted action sequences are actually applicable. If validity is reasonable (>50%), Phase 2 can take over.

If Phase 1 needs improvement before proceeding:
1. **More data** (most impactful) — generate additional rounds with new seeds
2. **Label smoothing** — `label_smoothing=0.1` on CrossEntropyLoss
3. **Increase dropout** — 0.1 → 0.2

### Restarting from Scratch

The AST structure and rule set are not finalized. Any change to `Expr` variants, rule definitions, or tokenization invalidates all training data and checkpoints. To rebuild everything:

```bash
# 1. Clean old artifacts
npm run ml:reset

# 2. Regenerate training data (3 rounds, ~14.7K examples)
npm run build:data

# 3. Retrain from scratch (100 epochs, ~1 hour on CPU)
npm run train
```

`npm run ml:reset` removes `data_training/`, `checkpoints/`, and any cached model artifacts. Always run this after changing AST nodes, rule definitions, tokenization logic, or the De Bruijn variable mapping.

## 10. M5 Results & Observations

### Validation Results (Phase 1 model, 1469 val examples)

| Metric | Value |
|--------|-------|
| Validity rate | 65.6% (963/1469 fully valid sequences) |
| Mean valid fraction | 65.8% |
| Mean complexity delta | +1.55 (simplification) |
| Improved | 737 (50.1%) |
| Unchanged | 732 |
| Worsened | 0 |
| Empty sequences | 0 |
| Mean steps | 6.9 |
| Validation time | 0.1s (Rust binary, all 1469 examples) |

### Key Insights

1. **Positions are step-relative** (resolves Open Question #1). The validator re-tokenizes after each rule application, so position indices refer to the *current* expression, not the initial one. This matches how `random_search.rs` generates training data. The model successfully learned step-relative positioning — 65.6% of sequences are fully valid.

2. **No worsened expressions.** When the model's sequence is valid, it always reduces or maintains complexity. This suggests the model learned the direction of simplification well, even when it makes invalid moves partway through.

3. **Direct validation via function call.** `validate_action_sequence()` is called directly in Rust, eliminating the subprocess overhead entirely.

4. **65.6% validity exceeds the threshold for Phase 2.** The design doc target for proceeding to RL was >50% validity. The model has learned enough structure that REINFORCE/PPO can refine it.

5. **The RuleDirectionId reverse lookup** (direction_id → rule_index + direction) was the main missing piece in `IndexedRuleSet`. LTR IDs are `0..num_rules`, RTL IDs are `num_rules..total_directions`, stored as a flat `Vec<(usize, Direction)>` for O(1) lookup.

## 11. M6 Results & Observations

### REINFORCE Training (3-epoch smoke test, MPS)

| Metric | Epoch 1 | Epoch 2 | Epoch 3 |
|--------|---------|---------|---------|
| Mean reward | +0.889 | +1.281 | +1.335 |
| Validity (sampling) | 48.9% | 52.9% | 51.7% |
| Loss | -1.17 | -1.25 | -0.96 |
| Time per epoch | 483s | 694s | 679s |

For comparison, Phase 1 greedy evaluation (M5): mean reward +0.30, validity 65.6%.

The lower sampling validity (49-53%) vs greedy validity (65.6%) is expected — sampling explores stochastically while greedy always picks the most likely token. The reward metric is more meaningful for RL: it increased from +0.889 to +1.335 across 3 epochs, indicating the model is learning to generate better simplification trajectories.

### Algorithm: REINFORCE with Baseline

- **Baseline**: Exponential moving average of rewards (decay 0.99) for variance reduction
- **Advantage normalization**: Per-batch standard deviation normalization prevents loss magnitude drift
- **Log-prob normalization**: Trajectory log-probs divided by sequence length to decouple loss scale from sequence length
- **Entropy bonus** (0.05): Prevents policy collapse — without this, the model degenerates to deterministic outputs within 1 epoch and crashes with NaN
- **Logit clamping** (-50, 50): Additional numerical safety in `sample()` to prevent inf/nan after many gradient updates

### Key Insights

1. **Entropy collapse is the primary failure mode.** The initial implementation (entropy_bonus=0.01, no advantage normalization) trained well for 1 epoch but collapsed to a degenerate policy at epoch 2 boundary — all rewards dropped to 0 and logits became NaN. Increasing entropy bonus to 0.05 and normalizing advantages per batch solved this completely.

2. **Log-prob normalization by sequence length is important.** Without it, the REINFORCE loss magnitude scales with sequence length (longer trajectories have more negative log-probs), causing the loss to grow unboundedly and dominate the gradient signal.

3. **GPU vs CPU for RL training.** With the subprocess bottleneck eliminated, the main bottleneck is the validation loop (CPU-bound Rust code). GPU (wgpu) and CPU (ndarray) perform similarly for RL training. GPU may cause thermal issues on laptops during long runs; ndarray is recommended for stability.

4. **Device selection** via `--device wgpu` (GPU) or `--device ndarray` (CPU). Use ndarray for long RL training runs on laptops.

5. **Reward structure works well.** The formula `delta / max(valid_steps, 1) - invalid_steps * 0.5` provides a clear learning signal: the model first learns to stop generating invalid actions (reducing penalty), then learns to generate sequences that actually simplify (increasing delta).

### Restarting RL from Scratch

```bash
# Full pipeline: supervised warm start → RL fine-tuning
npm run train          # Phase 1: supervised (saves checkpoints/best.mpk)
npm run rl-train       # Phase 2: REINFORCE (saves checkpoints/rl_best.mpk, rl_latest.mpk)
npm run evaluate -- --checkpoint checkpoints/rl_best  # Evaluate RL model
```

RL training automatically resumes from `rl_best.mpk` if it exists, restoring epoch, baseline, and best reward. To start RL from scratch, delete `checkpoints/rl_best.mpk` and `rl_best_metadata.json`.

## 12. Open Questions (updated)

1. ~~**Position stability across transformations**~~: Resolved — positions are step-relative. The validator re-tokenizes after each rule application, matching training data generation. The model learned this successfully (65.6% fully valid sequences).

2. **Attention over trees vs sequences**: Would a tree-structured attention pattern (children attend to parents and siblings) outperform flat positional attention?

3. **Rule composition**: Should we allow the model to predict "macro actions" — learned sequences of rules that commonly occur together (e.g., commute+assoc+factor)?

4. **Complexity as reward signal**: The current complexity metric (node count) is crude. Should we learn a complexity function that better reflects human notions of "simplified"?

5. **Tensor index handling**: Tensor expressions with dummy indices add combinatorial complexity. Should tensor simplification use a separate specialized model?

6. **Beam search in the decoder**: Should we use beam search decoding (exploring multiple action sequences in parallel) to improve the quality of predicted plans? This is decoder-level beam search, distinct from the expression-level beam search used for data generation.

7. **RL training duration**: The 3-epoch smoke test showed clear improvement but reward may not have converged. A longer run (20-50 epochs) on CPU would reveal the ceiling. Is the reward plateauing or still climbing?

8. **PPO vs REINFORCE**: REINFORCE works but has high variance. PPO's clipped objective could enable larger learning rates and faster convergence. Worth trying if REINFORCE plateaus.

## 13. Milestones

| Phase | Deliverable | Scope |
|-------|------------|-------|
| **M1** | Tokenizer + position mapper | Rust: `Expr ↔ tokens`, AST path ↔ pre-order index, De Bruijn canonicalization | **Done** — `token.rs` |
| **M2** | Randomized beam search | Rust: shuffled rule order, stochastic selection, multi-run traces | **Done** — `random_search.rs` |
| **M3** | Training data generator | Rust: random AST + trace export to JSON with position annotations | **Done** — `gen_expr.rs`, `training_data.rs`, `bin/gen_data.rs` |
| **M4** | Supervised baseline model | Rust: Burn transformer encoder-decoder trained on beam search traces | **Done** — `verso_training/src/model.rs`, `train.rs` |
| **M5** | Sequence validation harness | Rust: apply predicted action sequences, compute reward via direct function calls | **Done** — `validate.rs`, `verso_training/src/evaluate.rs` |
| **M6** | Self-play training | Rust: REINFORCE with Burn autograd and direct validation | **Done** — `verso_training/src/rl_train.rs` |
| **M7** | ~~ONNX inference~~ | Native Rust inference — free with Burn (no export needed) | **Done** |
| **M8** | Production integration | Rust: ML simplify with beam search fallback | **Done** — `verso_training/src/ml_simplify.rs`, `bin/ml_repl.rs` |
