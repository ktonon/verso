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

Use ONNX Runtime via the `ort` crate to run the trained model in Rust:

```rust
fn ml_simplify(expr: &Expr, model: &OrtSession, rules: &[Rule]) -> (Expr, Vec<TraceStep>) {
    let tokens = tokenize(expr);
    let action_sequence = model.generate(&tokens);  // decoder outputs full sequence

    let mut current = expr.clone();
    let mut trace = vec![TraceStep { expr: current.clone(), rule_name: None, rule_display: None }];

    for action in &action_sequence {
        if action.is_stop() { break; }
        let rule = &rules[action.rule_index()];
        let subexpr = subexpr_at(&current, action.position());
        let result = if action.is_ltr() {
            rule.apply_ltr(&subexpr)
        } else {
            rule.apply_rtl(&subexpr)
        };
        match result {
            Some(new_subexpr) => {
                current = replace_subexpr(&current, action.position(), new_subexpr);
                trace.push(TraceStep {
                    expr: current.clone(),
                    rule_name: Some(rule.name.clone()),
                    rule_display: Some(format!("{}", rule)),
                });
            }
            None => break,  // invalid action, stop applying
        }
    }
    (current, trace)
}
```

### Hybrid Strategy

For production use, run the ML model first (fast, usually correct), then fall back to beam search if the ML result has higher complexity than the input:

```rust
fn simplify(expr: &Expr, model: &OrtSession, rules: &RuleSet) -> Expr {
    let (ml_result, _trace) = ml_simplify(expr, model, &rules.all_rules());
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

- Python + PyTorch for model development
- Custom data loader that reads trace files (JSON or binary)
- Single GPU sufficient for the model size (~1-2M parameters)
- Training time: hours, not days

### Data Pipeline

```
┌────────────┐     ┌──────────────────┐     ┌─────────────┐     ┌──────────┐
│ Random AST │────▶│ Randomized Beam  │────▶│ Trace Export │────▶│ Training │
│ Generator  │     │ Search (×N runs) │     │ (JSON/bin)   │     │ (Python) │
└────────────┘     └──────────────────┘     └─────────────┘     └──────────┘
       │                                                              │
       │                  ┌───────────┐                               │
       └─────────────────▶│ ONNX      │◀──────────────────────────────┘
                          │ Export    │
                          └─────┬─────┘
                                │
                          ┌─────▼─────┐
                          │ Rust      │
                          │ Inference │
                          └───────────┘
```

### New Rust Components

1. **Tokenizer**: `Expr → Vec<Token>` and reverse, with De Bruijn variable canonicalization
2. **Position mapper**: pre-order index ↔ AST path, `subexpr_at()` and `replace_subexpr()`
3. **Random expression generator**: for training data, with configurable depth/type distributions
4. **Randomized beam search**: shuffled rule order, stochastic beam selection, multi-run
5. **Trace exporter**: serialize traces with position annotations to JSON/binary
6. **ONNX inference wrapper**: load model, run encoder+decoder, decode action sequence

### New Python Components

1. **Data loader**: read trace files, create batches with padding
2. **Model definition**: transformer encoder-decoder
3. **Training loop**: supervised (Phase 1) + REINFORCE/PPO (Phase 2)
4. **Validation harness**: call Rust rule engine to validate predicted sequences
5. **ONNX export**: save trained model for Rust consumption

## 9. Open Questions

1. **Position stability across transformations**: Positions refer to the initial AST, but rules change the tree structure. Should we re-tokenize after each action and use step-relative positions? This is more accurate but makes the output vocabulary step-dependent.

2. **Attention over trees vs sequences**: Would a tree-structured attention pattern (children attend to parents and siblings) outperform flat positional attention?

3. **Rule composition**: Should we allow the model to predict "macro actions" — learned sequences of rules that commonly occur together (e.g., commute+assoc+factor)?

4. **Complexity as reward signal**: The current complexity metric (node count) is crude. Should we learn a complexity function that better reflects human notions of "simplified"?

5. **Tensor index handling**: Tensor expressions with dummy indices add combinatorial complexity. Should tensor simplification use a separate specialized model?

6. **Beam search in the decoder**: Should we use beam search decoding (exploring multiple action sequences in parallel) to improve the quality of predicted plans? This is decoder-level beam search, distinct from the expression-level beam search used for data generation.

## 10. Milestones

| Phase | Deliverable | Scope |
|-------|------------|-------|
| **M1** | Tokenizer + position mapper | Rust: `Expr ↔ tokens`, AST path ↔ pre-order index, De Bruijn canonicalization | **Done** — `token.rs` |
| **M2** | Randomized beam search | Rust: shuffled rule order, stochastic selection, multi-run traces | **Done** — `random_search.rs` |
| **M3** | Training data generator | Rust: random AST + trace export to JSON with position annotations | **Done** — `gen_expr.rs`, `training_data.rs`, `bin/gen_data.rs` |
| **M4** | Supervised baseline model | Python: transformer encoder-decoder trained on beam search traces | **Done** — `erd_training/` |
| **M5** | Sequence validation harness | Rust/Python: apply predicted action sequences, compute reward |
| **M6** | Self-play training | Python: REINFORCE/PPO with trajectory reward |
| **M7** | ONNX inference in Rust | Rust: load model, generate action sequences, apply and validate |
| **M8** | Production integration | Rust: ML simplify with beam search fallback |
