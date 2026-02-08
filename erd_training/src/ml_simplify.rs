use burn::prelude::*;

use erd_symbolic::random_search::IndexedRuleSet;
use erd_symbolic::token::tokenize;
use erd_symbolic::training_data::token_to_string;
use erd_symbolic::validate::{validate_with_trace, ValidationResult};
use erd_symbolic::{simplify_with_trace, Expr, RuleSet, TraceStep};

use crate::config::TrainConfig;
use crate::evaluate::decode_action_sequence;
use crate::model::SimplificationModel;
use crate::train::load_model;
use crate::vocab::{DecoderVocab, EncoderVocab};

/// ML-powered simplification context.
///
/// Holds the loaded model, vocabularies, and rules needed for inference.
pub struct MLSimplifier<B: Backend> {
    model: SimplificationModel<B>,
    enc_vocab: EncoderVocab,
    dec_vocab: DecoderVocab,
    rules: IndexedRuleSet,
    device: B::Device,
}

impl<B: Backend> MLSimplifier<B> {
    /// Load a model from a checkpoint path (e.g. "checkpoints/rl_best").
    pub fn load(checkpoint: &str, device: B::Device) -> Self {
        let rules = IndexedRuleSet::new(RuleSet::full());
        let enc_vocab = EncoderVocab::new(&rules);
        let dec_vocab = DecoderVocab::new(&rules, 64);

        let config = TrainConfig::default_for_inference();
        let model: SimplificationModel<B> = load_model(
            &config,
            enc_vocab.size(),
            dec_vocab.size(),
            &device,
            checkpoint,
        );

        MLSimplifier {
            model,
            enc_vocab,
            dec_vocab,
            rules,
            device,
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
                let (beam_expr, trace) =
                    simplify_with_trace(expr, &RuleSet::full());
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

    /// Run ML inference: tokenize → encode → generate → decode → validate.
    pub fn run_inference(&self, expr: &Expr) -> (ValidationResult, Vec<TraceStep>) {
        let (tokens, _db) = tokenize(expr);
        let token_strings: Vec<String> = tokens.iter().map(token_to_string).collect();

        // Encode to tensor (batch of 1)
        let enc_ids_vec: Vec<i64> = token_strings
            .iter()
            .map(|t| self.enc_vocab.encode(t) as i64)
            .collect();
        let seq_len = enc_ids_vec.len();

        let enc_ids = Tensor::<B, 2, Int>::from_data(
            TensorData::new(enc_ids_vec, [1, seq_len]),
            &self.device,
        );
        let enc_pad_mask = enc_ids.clone().equal_elem(0);

        // Generate action sequence (greedy)
        let generated = self.model.generate(
            enc_ids,
            101,
            Some(enc_pad_mask),
            DecoderVocab::STOP as i64,
        );

        // Decode and validate
        let action_ids = &generated[0];
        let actions = decode_action_sequence(action_ids, &self.dec_vocab);

        validate_with_trace(expr, &actions, &self.rules)
    }
}
