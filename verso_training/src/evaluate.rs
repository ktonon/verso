use verso_symbolic::token::detokenize;
use verso_symbolic::training_data::{parse_token_string, synthetic_debruijn};
use verso_symbolic::validate::ValidationResult;

/// Reconstruct an Expr from input token strings.
pub fn tokens_to_expr(
    input_tokens: &[String],
) -> Result<verso_symbolic::Expr, String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use verso_symbolic::validate::ValidationResult;

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
            final_expr: verso_symbolic::Expr::new(verso_symbolic::ExprKind::Rational(
                verso_symbolic::rational::Rational::from_i64(1),
            )),
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
            final_expr: verso_symbolic::Expr::new(verso_symbolic::ExprKind::Rational(
                verso_symbolic::rational::Rational::from_i64(1),
            )),
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
            final_expr: verso_symbolic::Expr::new(verso_symbolic::ExprKind::Rational(
                verso_symbolic::rational::Rational::ZERO,
            )),
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
                final_expr: verso_symbolic::Expr::new(verso_symbolic::ExprKind::Rational(
                    verso_symbolic::rational::Rational::from_i64(1),
                )),
                final_complexity: 1,
                input_complexity: 5,
                step_details: Vec::new(),
            },
            ValidationResult {
                valid_steps: 0,
                total_steps: 1,
                final_expr: verso_symbolic::Expr::new(verso_symbolic::ExprKind::Rational(
                    verso_symbolic::rational::Rational::from_i64(3),
                )),
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
