use burn::prelude::*;

use verso_symbolic::random_search::IndexedRuleSet;
use verso_symbolic::validate::ValidationResult;
use verso_symbolic::{simplify_with_trace, Expr, RuleSet, TraceStep};

use crate::config::PolicyConfig;
use crate::policy_evaluate::policy_inference_loop;
use crate::policy_model::PolicyModel;
use crate::policy_train::load_policy_model;
use crate::vocab::EncoderVocab;

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
            None => {
                let (beam_expr, trace) = simplify_with_trace(expr, &RuleSet::full());
                (beam_expr, trace, false)
            }
        }
    }

    /// Simplify using ML only (no fallback).
    ///
    /// Returns `None` if the ML result doesn't improve on the input.
    pub fn simplify_ml_only(&self, expr: &Expr) -> Option<(Expr, Vec<TraceStep>)> {
        let (result, trace) = self.run_inference(expr);

        if result.final_complexity < result.input_complexity {
            Some((result.final_expr, trace))
        } else {
            None
        }
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
