use burn::prelude::*;

use verso_symbolic::random_search::IndexedRuleSet;
use verso_symbolic::validate::ValidationResult;
use verso_symbolic::{simplify_with_trace, Expr, RuleSet, TraceStep};

use crate::config::PolicyConfig;
use crate::policy_evaluate::policy_inference_loop;
use crate::policy_model::PolicyModel;
use crate::policy_train::load_policy_model;
use crate::vocab::EncoderVocab;

fn ml_result_improves(result: &ValidationResult) -> bool {
    result.final_complexity < result.input_complexity
}

fn select_ml_only_result(
    result: ValidationResult,
    trace: Vec<TraceStep>,
) -> Option<(Expr, Vec<TraceStep>)> {
    if ml_result_improves(&result) {
        Some((result.final_expr, trace))
    } else {
        None
    }
}

fn beam_fallback(expr: &Expr) -> (Expr, Vec<TraceStep>, bool) {
    let (beam_expr, trace) = simplify_with_trace(expr, &RuleSet::full());
    (beam_expr, trace, false)
}

/// ML-powered simplification context.
///
/// Uses a single-step policy model that re-encodes at each step.
pub struct MLSimplifier<B: Backend> {
    model: PolicyModel<B>,
    enc_vocab: EncoderVocab,
    rules: IndexedRuleSet,
    device: B::Device,
    max_steps: usize,
}

impl<B: Backend> MLSimplifier<B> {
    /// Load a policy model from a checkpoint path (e.g. "checkpoints/policy_rl_best").
    pub fn load(checkpoint: &str, device: B::Device) -> Self {
        let rules = IndexedRuleSet::new(RuleSet::full());
        let enc_vocab = EncoderVocab::new(&rules);
        let num_rules = rules.total_directions as usize;

        let config = PolicyConfig::default();
        let model: PolicyModel<B> =
            load_policy_model(&config, enc_vocab.size(), num_rules, &device, checkpoint);

        MLSimplifier {
            model,
            enc_vocab,
            rules,
            device,
            max_steps: 20,
        }
    }

    /// Simplify an expression using ML, with beam search fallback.
    ///
    /// Returns `(simplified_expr, trace, used_ml)` where `used_ml` indicates
    /// whether the ML model produced the result (true) or beam search was used (false).
    pub fn simplify(&self, expr: &Expr) -> (Expr, Vec<TraceStep>, bool) {
        match self.simplify_ml_only(expr) {
            Some((ml_expr, trace)) => (ml_expr, trace, true),
            None => beam_fallback(expr),
        }
    }

    /// Simplify using ML only (no fallback).
    ///
    /// Returns `None` if the ML result doesn't improve on the input.
    pub fn simplify_ml_only(&self, expr: &Expr) -> Option<(Expr, Vec<TraceStep>)> {
        let (result, trace) = self.run_inference(expr);
        select_ml_only_result(result, trace)
    }

    /// Run ML inference: multi-step predict → apply → re-encode loop.
    pub fn run_inference(&self, expr: &Expr) -> (ValidationResult, Vec<TraceStep>) {
        policy_inference_loop(
            &self.model,
            expr,
            &self.enc_vocab,
            &self.rules,
            self.max_steps,
            &self.device,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use verso_symbolic::parse_expr;

    fn trace_for(expr_text: &str) -> Vec<TraceStep> {
        vec![TraceStep {
            expr: parse_expr(expr_text).unwrap(),
            rule_name: Some("demo".to_string()),
            rule_display: None,
        }]
    }

    fn validation_result(
        final_expr: &str,
        final_complexity: usize,
        input_complexity: usize,
    ) -> ValidationResult {
        ValidationResult {
            valid_steps: 1,
            total_steps: 1,
            final_expr: parse_expr(final_expr).unwrap(),
            final_complexity,
            input_complexity,
            step_details: Vec::new(),
        }
    }

    #[test]
    fn select_ml_only_result_returns_some_when_ml_improves() {
        let result = validation_result("x", 1, 3);
        let trace = trace_for("x + 0");

        let selected = select_ml_only_result(result, trace);

        assert!(selected.is_some());
        let (expr, trace) = selected.unwrap();
        assert_eq!(expr, parse_expr("x").unwrap());
        assert_eq!(trace.len(), 1);
    }

    #[test]
    fn select_ml_only_result_returns_none_without_improvement() {
        let result = validation_result("x + 0", 3, 3);
        let trace = trace_for("x + 0");

        assert!(select_ml_only_result(result, trace).is_none());
    }

    #[test]
    fn beam_fallback_uses_search_and_marks_non_ml_result() {
        let expr = parse_expr("x + 0").unwrap();

        let (fallback_expr, trace, used_ml) = beam_fallback(&expr);

        assert!(!used_ml);
        assert_eq!(fallback_expr, parse_expr("x").unwrap());
        assert!(!trace.is_empty());
    }
}
