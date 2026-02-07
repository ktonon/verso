use burn::prelude::*;

use erd_symbolic::random_search::IndexedRuleSet;
use erd_symbolic::token::detokenize;
use erd_symbolic::training_data::{parse_token_string, synthetic_debruijn, TrainingExample};
use erd_symbolic::validate::{validate_action_sequence, PredictedAction, ValidationResult};

use crate::model::SimplificationModel;
use crate::vocab::{DecoderToken, DecoderVocab, EncoderVocab};

/// Decode model output token IDs into a sequence of PredictedActions.
///
/// Expects interleaved RULE/POS pairs. Stops at STOP token or malformed sequence.
pub fn decode_action_sequence(token_ids: &[i64], dec_vocab: &DecoderVocab) -> Vec<PredictedAction> {
    let mut actions = Vec::new();
    let mut i = 0;
    while i < token_ids.len() {
        match dec_vocab.decode(token_ids[i] as usize) {
            DecoderToken::Stop => break,
            DecoderToken::Rule(dir) => {
                if i + 1 < token_ids.len() {
                    if let DecoderToken::Pos(pos) = dec_vocab.decode(token_ids[i + 1] as usize) {
                        actions.push(PredictedAction {
                            rule_direction: dir as u16,
                            position: pos,
                        });
                        i += 2;
                        continue;
                    }
                }
                break; // malformed: RULE not followed by POS
            }
            _ => break, // unexpected token type
        }
    }
    actions
}

/// Reconstruct an Expr from input token strings.
pub fn tokens_to_expr(
    input_tokens: &[String],
) -> Result<erd_symbolic::Expr, String> {
    let tokens: Vec<_> = input_tokens
        .iter()
        .map(|s| parse_token_string(s))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Token parse error: {}", e))?;
    let db = synthetic_debruijn(&tokens);
    detokenize(&tokens, &db).map_err(|e| format!("Detokenize error: {:?}", e))
}

/// Compute reward for a single validation result.
pub fn compute_reward(result: &ValidationResult, invalid_penalty: f64) -> f64 {
    if result.total_steps == 0 {
        return 0.0;
    }
    let delta = result.input_complexity as f64 - result.final_complexity as f64;
    let invalid_steps = result.total_steps - result.valid_steps;
    let penalty = -(invalid_steps as f64) * invalid_penalty;
    delta / result.valid_steps.max(1) as f64 + penalty
}

/// Aggregated evaluation metrics.
#[derive(Debug, Clone)]
pub struct EvalMetrics {
    pub total_examples: usize,
    pub fully_valid_count: usize,
    pub validity_rate: f64,
    pub mean_valid_fraction: f64,
    pub mean_complexity_delta: f64,
    pub mean_reward: f64,
    pub improved_count: usize,
    pub unchanged_count: usize,
    pub worsened_count: usize,
    pub empty_sequence_count: usize,
    pub mean_steps: f64,
}

/// Compute metrics from a list of validation results.
pub fn compute_metrics(results: &[ValidationResult], invalid_penalty: f64) -> EvalMetrics {
    let total = results.len();
    if total == 0 {
        return EvalMetrics {
            total_examples: 0,
            fully_valid_count: 0,
            validity_rate: 0.0,
            mean_valid_fraction: 0.0,
            mean_complexity_delta: 0.0,
            mean_reward: 0.0,
            improved_count: 0,
            unchanged_count: 0,
            worsened_count: 0,
            empty_sequence_count: 0,
            mean_steps: 0.0,
        };
    }

    let mut fully_valid = 0usize;
    let mut valid_fraction_sum = 0.0f64;
    let mut delta_sum = 0.0f64;
    let mut reward_sum = 0.0f64;
    let mut improved = 0usize;
    let mut unchanged = 0usize;
    let mut worsened = 0usize;
    let mut empty = 0usize;
    let mut steps_sum = 0.0f64;

    for r in results {
        if r.total_steps == 0 {
            empty += 1;
            unchanged += 1;
            // valid_fraction is undefined for empty; treat as 1.0
            valid_fraction_sum += 1.0;
        } else {
            if r.valid_steps == r.total_steps {
                fully_valid += 1;
            }
            valid_fraction_sum += r.valid_steps as f64 / r.total_steps as f64;
        }

        let delta = r.input_complexity as f64 - r.final_complexity as f64;
        delta_sum += delta;
        reward_sum += compute_reward(r, invalid_penalty);
        steps_sum += r.total_steps as f64;

        if r.total_steps > 0 {
            if r.final_complexity < r.input_complexity {
                improved += 1;
            } else if r.final_complexity == r.input_complexity {
                unchanged += 1;
            } else {
                worsened += 1;
            }
        }
    }

    EvalMetrics {
        total_examples: total,
        fully_valid_count: fully_valid,
        validity_rate: fully_valid as f64 / total as f64,
        mean_valid_fraction: valid_fraction_sum / total as f64,
        mean_complexity_delta: delta_sum / total as f64,
        mean_reward: reward_sum / total as f64,
        improved_count: improved,
        unchanged_count: unchanged,
        worsened_count: worsened,
        empty_sequence_count: empty,
        mean_steps: steps_sum / total as f64,
    }
}

/// Print evaluation metrics.
pub fn print_metrics(metrics: &EvalMetrics) {
    println!("\n=== Evaluation Results ===");
    println!("Total examples:       {}", metrics.total_examples);
    println!("Fully valid:          {} ({:.1}%)", metrics.fully_valid_count, metrics.validity_rate * 100.0);
    println!("Mean valid fraction:  {:.4}", metrics.mean_valid_fraction);
    println!("Mean complexity delta:{:.4}", metrics.mean_complexity_delta);
    println!("Mean reward:          {:.4}", metrics.mean_reward);
    println!("Mean steps:           {:.2}", metrics.mean_steps);
    println!("Improved:             {}", metrics.improved_count);
    println!("Unchanged:            {}", metrics.unchanged_count);
    println!("Worsened:             {}", metrics.worsened_count);
    println!("Empty sequences:      {}", metrics.empty_sequence_count);
}

/// Run evaluation: generate predictions, validate, compute metrics.
pub fn evaluate<B: Backend>(
    model: &SimplificationModel<B>,
    val_examples: &[TrainingExample],
    enc_vocab: &EncoderVocab,
    dec_vocab: &DecoderVocab,
    rules: &IndexedRuleSet,
    batch_size: usize,
    invalid_penalty: f64,
    device: &B::Device,
) -> EvalMetrics {
    let mut all_results: Vec<ValidationResult> = Vec::new();
    let mut parse_errors = 0usize;

    for chunk in val_examples.chunks(batch_size) {
        // Encode input tokens to IDs
        let enc_id_seqs: Vec<Vec<i64>> = chunk
            .iter()
            .map(|ex| {
                ex.input_tokens
                    .iter()
                    .map(|t| enc_vocab.encode(t) as i64)
                    .collect()
            })
            .collect();

        // Pad to max length in batch
        let max_enc_len = enc_id_seqs.iter().map(|s| s.len()).max().unwrap_or(0);
        let batch_len = chunk.len();

        let flat: Vec<i64> = enc_id_seqs
            .iter()
            .flat_map(|s| {
                let mut padded = s.clone();
                padded.resize(max_enc_len, 0);
                padded
            })
            .collect();

        let enc_ids = Tensor::<B, 2, Int>::from_data(
            TensorData::new(flat, [batch_len, max_enc_len]),
            device,
        );
        let enc_pad_mask = enc_ids.clone().equal_elem(0);

        // Generate predictions (greedy)
        let generated = model.generate(enc_ids, 101, Some(enc_pad_mask), 2);

        // Validate each prediction
        for (i, token_ids) in generated.iter().enumerate() {
            let actions = decode_action_sequence(token_ids, dec_vocab);

            match tokens_to_expr(&chunk[i].input_tokens) {
                Ok(expr) => {
                    let result = validate_action_sequence(&expr, &actions, rules);
                    all_results.push(result);
                }
                Err(e) => {
                    parse_errors += 1;
                    eprintln!("Warning: could not parse example {}: {}", i, e);
                    // Push a zero-result so metrics still count this example
                    all_results.push(ValidationResult {
                        valid_steps: 0,
                        total_steps: actions.len(),
                        final_expr: erd_symbolic::Expr::Rational(
                            erd_symbolic::rational::Rational::ZERO,
                        ),
                        final_complexity: 0,
                        input_complexity: 0,
                        step_details: Vec::new(),
                    });
                }
            }
        }
    }

    if parse_errors > 0 {
        eprintln!("Warning: {} examples had parse errors", parse_errors);
    }

    compute_metrics(&all_results, invalid_penalty)
}

#[cfg(test)]
mod tests {
    use super::*;
    use erd_symbolic::random_search::IndexedRuleSet;
    use erd_symbolic::validate::ValidationResult;
    use erd_symbolic::RuleSet;

    #[test]
    fn test_decode_action_sequence_basic() {
        let indexed = IndexedRuleSet::new(RuleSet::full());
        let dec_vocab = DecoderVocab::new(&indexed, 64);

        // Encode: RULE_5, POS_3, STOP
        let token_ids = vec![
            dec_vocab.encode_rule(5) as i64,
            dec_vocab.encode_pos(3) as i64,
            DecoderVocab::STOP as i64,
        ];

        let actions = decode_action_sequence(&token_ids, &dec_vocab);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].rule_direction, 5);
        assert_eq!(actions[0].position, 3);
    }

    #[test]
    fn test_decode_action_sequence_multiple() {
        let indexed = IndexedRuleSet::new(RuleSet::full());
        let dec_vocab = DecoderVocab::new(&indexed, 64);

        let token_ids = vec![
            dec_vocab.encode_rule(2) as i64,
            dec_vocab.encode_pos(0) as i64,
            dec_vocab.encode_rule(10) as i64,
            dec_vocab.encode_pos(1) as i64,
            DecoderVocab::STOP as i64,
        ];

        let actions = decode_action_sequence(&token_ids, &dec_vocab);
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].rule_direction, 2);
        assert_eq!(actions[0].position, 0);
        assert_eq!(actions[1].rule_direction, 10);
        assert_eq!(actions[1].position, 1);
    }

    #[test]
    fn test_decode_action_sequence_empty_stop() {
        let indexed = IndexedRuleSet::new(RuleSet::full());
        let dec_vocab = DecoderVocab::new(&indexed, 64);

        let token_ids = vec![DecoderVocab::STOP as i64];
        let actions = decode_action_sequence(&token_ids, &dec_vocab);
        assert_eq!(actions.len(), 0);
    }

    #[test]
    fn test_decode_action_sequence_malformed() {
        let indexed = IndexedRuleSet::new(RuleSet::full());
        let dec_vocab = DecoderVocab::new(&indexed, 64);

        // RULE not followed by POS
        let token_ids = vec![
            dec_vocab.encode_rule(5) as i64,
            dec_vocab.encode_rule(3) as i64, // another RULE, not POS
        ];

        let actions = decode_action_sequence(&token_ids, &dec_vocab);
        assert_eq!(actions.len(), 0);
    }

    #[test]
    fn test_tokens_to_expr() {
        let tokens = vec!["ADD".to_string(), "I_1".to_string(), "I_2".to_string()];
        let expr = tokens_to_expr(&tokens).unwrap();
        assert_eq!(expr.complexity(), 3);
    }

    #[test]
    fn test_compute_reward_improved() {
        let result = ValidationResult {
            valid_steps: 2,
            total_steps: 2,
            final_expr: erd_symbolic::Expr::Rational(
                erd_symbolic::rational::Rational::from_i64(1),
            ),
            final_complexity: 1,
            input_complexity: 5,
            step_details: Vec::new(),
        };
        let reward = compute_reward(&result, 0.5);
        // delta=4, valid_steps=2, no penalty → 4/2 = 2.0
        assert!((reward - 2.0).abs() < 1e-10);
    }

    #[test]
    fn test_compute_reward_with_invalid() {
        let result = ValidationResult {
            valid_steps: 1,
            total_steps: 3,
            final_expr: erd_symbolic::Expr::Rational(
                erd_symbolic::rational::Rational::from_i64(1),
            ),
            final_complexity: 3,
            input_complexity: 5,
            step_details: Vec::new(),
        };
        let reward = compute_reward(&result, 0.5);
        // delta=2, valid_steps=1, invalid=2, penalty=-1.0 → 2/1 - 1.0 = 1.0
        assert!((reward - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_compute_reward_empty() {
        let result = ValidationResult {
            valid_steps: 0,
            total_steps: 0,
            final_expr: erd_symbolic::Expr::Rational(
                erd_symbolic::rational::Rational::ZERO,
            ),
            final_complexity: 5,
            input_complexity: 5,
            step_details: Vec::new(),
        };
        assert_eq!(compute_reward(&result, 0.5), 0.0);
    }

    #[test]
    fn test_compute_metrics() {
        let results = vec![
            ValidationResult {
                valid_steps: 2,
                total_steps: 2,
                final_expr: erd_symbolic::Expr::Rational(
                    erd_symbolic::rational::Rational::from_i64(1),
                ),
                final_complexity: 1,
                input_complexity: 5,
                step_details: Vec::new(),
            },
            ValidationResult {
                valid_steps: 0,
                total_steps: 1,
                final_expr: erd_symbolic::Expr::Rational(
                    erd_symbolic::rational::Rational::from_i64(3),
                ),
                final_complexity: 3,
                input_complexity: 3,
                step_details: Vec::new(),
            },
        ];
        let metrics = compute_metrics(&results, 0.5);
        assert_eq!(metrics.total_examples, 2);
        assert_eq!(metrics.fully_valid_count, 1);
        assert_eq!(metrics.improved_count, 1);
        assert_eq!(metrics.worsened_count, 0);
    }
}
