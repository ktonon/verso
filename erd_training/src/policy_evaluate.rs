use burn::prelude::*;

use erd_symbolic::random_search::{Direction, IndexedRuleSet, RuleDirectionId};
use erd_symbolic::token::tokenize;
use erd_symbolic::training_data::token_to_string;
use erd_symbolic::validate::{validate_action_sequence, PredictedAction, ValidationResult};
use erd_symbolic::{Expr, TraceStep};

use crate::config::PolicyConfig;
use crate::evaluate::{compute_metrics, tokens_to_expr, EvalMetrics};
use crate::policy_model::PolicyModel;
use crate::policy_train::load_policy_model;
use crate::vocab::EncoderVocab;

use erd_symbolic::training_data::TrainingExample;

/// Run policy inference loop: repeatedly predict and apply single actions.
///
/// Returns (ValidationResult, Vec<TraceStep>) for the full multi-step episode.
pub fn policy_inference_loop<B: Backend>(
    model: &PolicyModel<B>,
    expr: &Expr,
    enc_vocab: &EncoderVocab,
    rules: &IndexedRuleSet,
    max_steps: usize,
    device: &B::Device,
) -> (ValidationResult, Vec<TraceStep>) {
    let input_complexity = expr.complexity();
    let mut current_expr = expr.clone();
    let mut best_expr = expr.clone();
    let mut best_complexity = input_complexity;
    let mut trace = vec![TraceStep {
        expr: expr.clone(),
        rule_name: None,
        rule_display: None,
    }];
    let mut valid_steps = 0;
    let mut total_steps = 0;
    let mut step_details = Vec::new();

    for _ in 0..max_steps {
        // Tokenize current expression and encode
        let (tokens, _db) = tokenize(&current_expr);
        let enc_ids_vec: Vec<i64> = tokens
            .iter()
            .map(|t| enc_vocab.encode(&token_to_string(t)) as i64)
            .collect();
        let seq_len = enc_ids_vec.len();

        let enc_ids = Tensor::<B, 2, Int>::from_data(
            TensorData::new(enc_ids_vec, [1, seq_len]),
            device,
        );
        let enc_pad_mask = enc_ids.clone().equal_elem(0);

        // Predict single action
        let predictions = model.predict(enc_ids, enc_pad_mask);
        let (rule_dir, position) = predictions[0];

        total_steps += 1;

        // Validate and apply
        let predicted = PredictedAction {
            rule_direction: rule_dir as u16,
            position,
        };
        let result = validate_action_sequence(&current_expr, &[predicted], rules);

        if result.valid_steps == 0 {
            step_details.extend(result.step_details);
            break; // Invalid action, stop
        }

        valid_steps += 1;
        step_details.extend(result.step_details);

        // Get rule name for trace
        let rule_name = rules
            .lookup_direction(RuleDirectionId(rule_dir as u16))
            .map(|(idx, dir)| {
                let rule = rules.rule(idx);
                let dir_str = match dir {
                    Direction::Ltr => "->",
                    Direction::Rtl => "<-",
                };
                format!("{} {}", rule.name, dir_str)
            });

        current_expr = result.final_expr;
        let current_complexity = current_expr.complexity();

        trace.push(TraceStep {
            expr: current_expr.clone(),
            rule_name,
            rule_display: None,
        });

        if current_complexity < best_complexity {
            best_expr = current_expr.clone();
            best_complexity = current_complexity;
        } else {
            // No improvement — stop to avoid wasting steps
            break;
        }
    }

    let result = ValidationResult {
        valid_steps,
        total_steps,
        final_expr: best_expr,
        final_complexity: best_complexity,
        input_complexity,
        step_details,
    };

    (result, trace)
}

/// Run evaluation on validation examples using the policy model.
pub fn policy_evaluate<B: Backend>(
    model: &PolicyModel<B>,
    val_examples: &[TrainingExample],
    enc_vocab: &EncoderVocab,
    rules: &IndexedRuleSet,
    max_steps: usize,
    invalid_penalty: f64,
    device: &B::Device,
) -> EvalMetrics {
    let mut all_results: Vec<ValidationResult> = Vec::new();
    let mut parse_errors = 0usize;

    for (i, example) in val_examples.iter().enumerate() {
        match tokens_to_expr(&example.input_tokens) {
            Ok(expr) => {
                let (result, _trace) =
                    policy_inference_loop(model, &expr, enc_vocab, rules, max_steps, device);
                all_results.push(result);
            }
            Err(e) => {
                parse_errors += 1;
                eprintln!("Warning: could not parse example {}: {}", i, e);
                all_results.push(ValidationResult {
                    valid_steps: 0,
                    total_steps: 0,
                    final_expr: Expr::Rational(erd_symbolic::rational::Rational::ZERO),
                    final_complexity: 0,
                    input_complexity: 0,
                    step_details: Vec::new(),
                });
            }
        }
    }

    if parse_errors > 0 {
        eprintln!("Warning: {} examples had parse errors", parse_errors);
    }

    compute_metrics(&all_results, invalid_penalty)
}

/// CLI configuration for policy evaluation.
#[derive(clap::Parser, Debug, Clone)]
pub struct PolicyEvalConfig {
    #[arg(long, default_value = "checkpoints/policy_best")]
    pub checkpoint: String,
    #[arg(long, default_value = "data_training")]
    pub data_dir: String,
    #[arg(long, default_value_t = 0.1)]
    pub val_fraction: f64,
    #[arg(long, default_value_t = 42)]
    pub seed: u64,
    #[arg(long, default_value_t = 0.5)]
    pub invalid_penalty: f64,
    #[arg(long, default_value_t = 20)]
    pub max_steps: usize,
    #[arg(long, default_value = "cpu")]
    pub device: String,

    // Model architecture (must match training config)
    #[arg(long, default_value_t = 128)]
    pub d_model: usize,
    #[arg(long, default_value_t = 4)]
    pub n_encoder_layers: usize,
    #[arg(long, default_value_t = 4)]
    pub n_heads: usize,
    #[arg(long, default_value_t = 256)]
    pub d_ff: usize,
    #[arg(long, default_value_t = 0.1)]
    pub dropout: f64,
    #[arg(long, default_value_t = 64)]
    pub max_enc_len: usize,
}

impl PolicyEvalConfig {
    pub fn to_policy_config(&self) -> PolicyConfig {
        PolicyConfig {
            d_model: self.d_model,
            n_encoder_layers: self.n_encoder_layers,
            n_heads: self.n_heads,
            d_ff: self.d_ff,
            dropout: self.dropout,
            max_enc_len: self.max_enc_len,
        }
    }
}

/// Run the full policy evaluation pipeline.
pub fn run_policy_evaluation<B: Backend>(config: PolicyEvalConfig, device: B::Device) {
    let rules = IndexedRuleSet::new(erd_symbolic::RuleSet::full());
    let enc_vocab = EncoderVocab::new(&rules);
    let num_rules = rules.total_directions as usize;

    println!("Loading data from {}...", config.data_dir);
    let (_train, val) = crate::dataset::load_data(&config.data_dir, config.val_fraction, config.seed);
    println!("Validation examples: {}", val.len());

    println!("Loading model from {}...", config.checkpoint);
    let policy_config = config.to_policy_config();
    let model: PolicyModel<B> = load_policy_model(
        &policy_config,
        enc_vocab.size(),
        num_rules,
        &device,
        &config.checkpoint,
    );

    println!("Evaluating (max {} steps per example)...", config.max_steps);
    let t0 = std::time::Instant::now();
    let metrics = policy_evaluate(
        &model,
        &val,
        &enc_vocab,
        &rules,
        config.max_steps,
        config.invalid_penalty,
        &device,
    );
    let elapsed = t0.elapsed();

    crate::evaluate::print_metrics(&metrics);
    println!("Evaluation time:      {:.1}s", elapsed.as_secs_f64());
}

#[cfg(test)]
mod tests {
    use super::*;
    use burn::backend::NdArray;
    use erd_symbolic::random_search::IndexedRuleSet;
    use erd_symbolic::RuleSet;

    type TestBackend = NdArray;

    #[test]
    fn test_inference_loop_produces_trace() {
        let indexed = IndexedRuleSet::new(RuleSet::full());
        let enc_vocab = EncoderVocab::new(&indexed);
        let num_rules = indexed.total_directions as usize;
        let config = PolicyConfig::default();
        let device = burn::backend::ndarray::NdArrayDevice::Cpu;
        let model: PolicyModel<TestBackend> =
            PolicyModel::new(&config, enc_vocab.size(), num_rules, &device);

        // Parse a simple expression: x + 0
        let expr = erd_symbolic::parse_expr("x + 0").unwrap();
        let (result, trace) =
            policy_inference_loop(&model, &expr, &enc_vocab, &indexed, 10, &device);

        // With random weights, the model may or may not produce valid actions,
        // but it should at least produce a trace with the initial expression
        assert!(!trace.is_empty());
        assert_eq!(trace[0].expr, expr);
        // total_steps should be at least 1 (it tries one action)
        assert!(result.total_steps >= 1);
    }

    #[test]
    fn test_inference_loop_stops_on_no_improvement() {
        let indexed = IndexedRuleSet::new(RuleSet::full());
        let enc_vocab = EncoderVocab::new(&indexed);
        let num_rules = indexed.total_directions as usize;
        let config = PolicyConfig::default();
        let device = burn::backend::ndarray::NdArrayDevice::Cpu;
        let model: PolicyModel<TestBackend> =
            PolicyModel::new(&config, enc_vocab.size(), num_rules, &device);

        // Simple constant — hard to improve on
        let expr = erd_symbolic::parse_expr("1").unwrap();
        let (result, _trace) =
            policy_inference_loop(&model, &expr, &enc_vocab, &indexed, 100, &device);

        // Should stop well before max_steps since there's nothing to improve
        assert!(result.total_steps <= 2);
    }

    #[test]
    fn test_policy_evaluate_with_examples() {
        let indexed = IndexedRuleSet::new(RuleSet::full());
        let enc_vocab = EncoderVocab::new(&indexed);
        let num_rules = indexed.total_directions as usize;
        let config = PolicyConfig::default();
        let device = burn::backend::ndarray::NdArrayDevice::Cpu;
        let model: PolicyModel<TestBackend> =
            PolicyModel::new(&config, enc_vocab.size(), num_rules, &device);

        let examples = vec![
            TrainingExample {
                input_tokens: vec!["ADD".to_string(), "V0".to_string(), "I_0".to_string()],
                actions: vec![],
                output_complexity: 1,
                input_complexity: 3,
            },
            TrainingExample {
                input_tokens: vec!["MUL".to_string(), "V0".to_string(), "I_1".to_string()],
                actions: vec![],
                output_complexity: 1,
                input_complexity: 3,
            },
        ];

        let metrics = policy_evaluate(&model, &examples, &enc_vocab, &indexed, 10, 0.5, &device);
        assert_eq!(metrics.total_examples, 2);
    }
}
