use burn::nn::transformer::{
    TransformerEncoder, TransformerEncoderConfig, TransformerEncoderInput,
};
use burn::nn::{Embedding, EmbeddingConfig, Linear, LinearConfig};
use burn::prelude::*;

use crate::config::PolicyConfig;

/// Single-step policy model for expression simplification.
///
/// Encoder-only transformer with two classification heads:
/// - Rule head: predicts which rule+direction to apply
/// - Position head: predicts which token position to apply it at
#[derive(Module, Debug)]
pub struct PolicyModel<B: Backend> {
    enc_tok_emb: Embedding<B>,
    enc_pos_emb: Embedding<B>,
    encoder: TransformerEncoder<B>,
    rule_head: Linear<B>,
    pos_head: Linear<B>,
}

impl<B: Backend> PolicyModel<B> {
    pub fn new(
        config: &PolicyConfig,
        enc_vocab_size: usize,
        num_rule_directions: usize,
        device: &B::Device,
    ) -> Self {
        let enc_tok_emb = EmbeddingConfig::new(enc_vocab_size, config.d_model).init(device);
        let enc_pos_emb = EmbeddingConfig::new(config.max_enc_len, config.d_model).init(device);
        let encoder = TransformerEncoderConfig::new(
            config.d_model,
            config.d_ff,
            config.n_heads,
            config.n_encoder_layers,
        )
        .with_dropout(config.dropout)
        .init(device);

        let rule_head = LinearConfig::new(config.d_model, num_rule_directions).init(device);
        let pos_head = LinearConfig::new(config.d_model, 1).init(device);

        Self {
            enc_tok_emb,
            enc_pos_emb,
            encoder,
            rule_head,
            pos_head,
        }
    }

    /// Forward pass: encode expression, produce rule and position logits.
    ///
    /// Args:
    ///   enc_ids: [B, S] encoder token IDs
    ///   enc_pad_mask: [B, S] true where padded
    ///
    /// Returns:
    ///   (rule_logits: [B, R], pos_logits: [B, S])
    pub fn forward(
        &self,
        enc_ids: Tensor<B, 2, Int>,
        enc_pad_mask: Tensor<B, 2, Bool>,
    ) -> (Tensor<B, 2>, Tensor<B, 2>) {
        let device = &enc_ids.device();
        let [batch_size, enc_len] = enc_ids.dims();

        // Encode
        let enc_pos = Tensor::<B, 1, Int>::arange(0..enc_len as i64, device)
            .unsqueeze_dim::<2>(0)
            .expand([batch_size, enc_len]);
        let enc_emb = self.enc_tok_emb.forward(enc_ids) + self.enc_pos_emb.forward(enc_pos);
        let hidden = self
            .encoder
            .forward(TransformerEncoderInput::new(enc_emb).mask_pad(enc_pad_mask.clone()));
        // hidden: [B, S, D]

        // Rule head: mean-pool over non-pad positions → [B, D] → [B, R]
        let d_model = hidden.dims()[2];
        let mask_float = enc_pad_mask.clone().bool_not().float(); // [B, S] — 1.0 for real, 0.0 for pad
        let mask_expanded = mask_float.clone().unsqueeze_dim(2); // [B, S, 1]
        let masked_hidden = hidden.clone() * mask_expanded; // [B, S, D]
        let summed = masked_hidden.sum_dim(1); // [B, 1, D]
        let counts = mask_float.sum_dim(1).unsqueeze_dim(2).clamp_min(1.0); // [B, 1, 1]
        let pooled = summed / counts; // [B, 1, D]
        let pooled = pooled.reshape([batch_size, d_model]); // [B, D]
        let rule_logits = self.rule_head.forward(pooled); // [B, R]

        // Position head: per-token logit → [B, S]
        let pos_scores = self.pos_head.forward(hidden); // [B, S, 1]
        let pos_logits = pos_scores.reshape([batch_size, enc_len]); // [B, S]
        // Mask pad positions to large negative value so they get ~zero probability.
        // Using -1e4 (not -inf or -1e9) to avoid blowing up CrossEntropyLoss
        // if a target accidentally lands on a masked position.
        let neg_large = Tensor::<B, 2>::full([batch_size, enc_len], -1e4, device);
        let pos_logits = pos_logits.mask_where(enc_pad_mask, neg_large);

        (rule_logits, pos_logits)
    }

    /// Greedy prediction: argmax on both heads.
    ///
    /// Returns Vec of (rule_direction_id, position) per batch element.
    pub fn predict(
        &self,
        enc_ids: Tensor<B, 2, Int>,
        enc_pad_mask: Tensor<B, 2, Bool>,
    ) -> Vec<(usize, usize)> {
        let (rule_logits, pos_logits) = self.forward(enc_ids, enc_pad_mask);
        let batch_size = rule_logits.dims()[0];

        let rule_ids = int_tensor_to_vec(rule_logits.argmax(1).reshape([batch_size]));
        let pos_ids = int_tensor_to_vec(pos_logits.argmax(1).reshape([batch_size]));

        rule_ids
            .into_iter()
            .zip(pos_ids)
            .map(|(r, p)| (r as usize, p as usize))
            .collect()
    }

    /// Stochastic sampling for RL.
    ///
    /// Returns Vec of (rule, position, log_p_rule, log_p_pos) per batch element.
    pub fn sample(
        &self,
        enc_ids: Tensor<B, 2, Int>,
        enc_pad_mask: Tensor<B, 2, Bool>,
        temperature: f64,
    ) -> Vec<(usize, usize, f64, f64)> {
        let (rule_logits, pos_logits) = self.forward(enc_ids, enc_pad_mask);
        let batch_size = rule_logits.dims()[0];

        // Sample rules
        let rule_logits = rule_logits.clamp(-50.0, 50.0) / temperature;
        let rule_probs = burn::tensor::activation::softmax(rule_logits.clone(), 1);
        let rule_ids = multinomial_sample(&rule_probs);
        let rule_log_probs = log_prob_at(&rule_logits, &rule_ids);

        // Sample positions
        let pos_logits = pos_logits.clamp(-50.0, 50.0) / temperature;
        let pos_probs = burn::tensor::activation::softmax(pos_logits.clone(), 1);
        let pos_ids = multinomial_sample(&pos_probs);
        let pos_log_probs = log_prob_at(&pos_logits, &pos_ids);

        let rule_ids = int_tensor_to_vec(Tensor::<B, 1, Int>::from_data(
            TensorData::new(rule_ids.clone(), [batch_size]),
            &rule_logits.device(),
        ));
        let pos_ids_i64 = int_tensor_to_vec(Tensor::<B, 1, Int>::from_data(
            TensorData::new(pos_ids.clone(), [batch_size]),
            &pos_logits.device(),
        ));

        rule_ids
            .into_iter()
            .zip(pos_ids_i64)
            .zip(rule_log_probs)
            .zip(pos_log_probs)
            .map(|(((r, p), lpr), lpp)| (r as usize, p as usize, lpr, lpp))
            .collect()
    }
}

/// Multinomial sampling from probability distributions [B, V].
/// Returns sampled indices as Vec<i64>.
fn multinomial_sample<B: Backend>(probs: &Tensor<B, 2>) -> Vec<i64> {
    let [batch_size, vocab_size] = probs.dims();
    let device = &probs.device();

    let cumsum = probs.clone().cumsum(1);
    let uniform = Tensor::<B, 2>::random(
        [batch_size, 1],
        burn::tensor::Distribution::Uniform(0.0, 1.0),
        device,
    );

    let ge = cumsum.greater_equal(uniform);
    let indices = Tensor::<B, 1, Int>::arange(0..vocab_size as i64, device)
        .float()
        .unsqueeze_dim::<2>(0)
        .expand([batch_size, vocab_size]);
    let large = Tensor::<B, 2>::full([batch_size, vocab_size], vocab_size as f32, device);
    let masked_indices = ge.clone().float().mul(indices) + ge.bool_not().float().mul(large);
    let sampled = masked_indices.argmin(1).reshape([batch_size]);

    int_tensor_to_vec(sampled)
}

/// Compute log probability at sampled indices.
/// logits: [B, V], indices: Vec<i64> of length B.
/// Returns Vec<f64> of log_softmax values at the chosen indices.
fn log_prob_at<B: Backend>(logits: &Tensor<B, 2>, indices: &[i64]) -> Vec<f64> {
    let log_softmax = burn::tensor::activation::log_softmax(logits.clone(), 1);
    let data = log_softmax.into_data();
    let [_batch_size, vocab_size] = logits.dims();

    // Extract values - try f32 first (most common), fall back
    let flat: Vec<f32> = data
        .to_vec::<f32>()
        .unwrap_or_else(|_| data.to_vec::<f64>().unwrap().into_iter().map(|x| x as f32).collect());

    indices
        .iter()
        .enumerate()
        .map(|(b, &idx)| flat[b * vocab_size + idx as usize] as f64)
        .collect()
}

/// Extract integer values from a 1D Int tensor.
fn int_tensor_to_vec<B: Backend>(tensor: Tensor<B, 1, Int>) -> Vec<i64> {
    let data = tensor.into_data();
    data.to_vec::<i64>().unwrap_or_else(|_| {
        data.to_vec::<i32>()
            .unwrap()
            .into_iter()
            .map(|x| x as i64)
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use burn::backend::NdArray;
    use erd_symbolic::random_search::IndexedRuleSet;
    use erd_symbolic::RuleSet;

    type TestBackend = NdArray;

    fn make_model() -> (PolicyModel<TestBackend>, usize, usize) {
        let indexed = IndexedRuleSet::new(RuleSet::full());
        let enc_vocab = crate::vocab::EncoderVocab::new(&indexed);
        let num_rules = indexed.total_directions as usize;
        let config = PolicyConfig::default();
        let device = burn::backend::ndarray::NdArrayDevice::Cpu;
        let model = PolicyModel::new(&config, enc_vocab.size(), num_rules, &device);
        (model, enc_vocab.size(), num_rules)
    }

    #[test]
    fn test_forward_shapes() {
        let (model, _, num_rules) = make_model();
        let device = burn::backend::ndarray::NdArrayDevice::Cpu;

        let batch_size = 2;
        let enc_len = 5;

        let enc_ids = Tensor::<TestBackend, 2, Int>::from_data(
            TensorData::new(vec![1i64; batch_size * enc_len], [batch_size, enc_len]),
            &device,
        );
        let enc_pad_mask = Tensor::<TestBackend, 2, Bool>::from_data(
            TensorData::from([[false; 5]; 2]),
            &device,
        );

        let (rule_logits, pos_logits) = model.forward(enc_ids, enc_pad_mask);
        assert_eq!(rule_logits.dims(), [batch_size, num_rules]);
        assert_eq!(pos_logits.dims(), [batch_size, enc_len]);
    }

    #[test]
    fn test_parameter_count() {
        let (model, _, _) = make_model();
        let num_params = model.num_params();
        println!("PolicyModel parameter count: {}", num_params);
        // Encoder-only + 2 small linear heads, should be ~600K-700K
        assert!(
            num_params > 500_000 && num_params < 800_000,
            "param count {} outside expected range [500K, 800K]",
            num_params,
        );
    }

    #[test]
    fn test_predict() {
        let (model, _, _) = make_model();
        let device = burn::backend::ndarray::NdArrayDevice::Cpu;

        let enc_ids = Tensor::<TestBackend, 2, Int>::from_data(
            TensorData::new(vec![1i64, 2, 3, 4, 5], [1, 5]),
            &device,
        );
        let enc_pad_mask = Tensor::<TestBackend, 2, Bool>::from_data(
            TensorData::from([[false; 5]]),
            &device,
        );

        let predictions = model.predict(enc_ids, enc_pad_mask);
        assert_eq!(predictions.len(), 1);
        let (rule, pos) = predictions[0];
        println!("Predicted rule={}, position={}", rule, pos);
        assert!(pos < 5); // position must be within input length
    }

    #[test]
    fn test_pad_masking() {
        let (model, _, _) = make_model();
        let device = burn::backend::ndarray::NdArrayDevice::Cpu;

        // Input with 3 real tokens + 2 pad
        let enc_ids = Tensor::<TestBackend, 2, Int>::from_data(
            TensorData::new(vec![1i64, 2, 3, 0, 0], [1, 5]),
            &device,
        );
        let enc_pad_mask = Tensor::<TestBackend, 2, Bool>::from_data(
            TensorData::from([[false, false, false, true, true]]),
            &device,
        );

        let predictions = model.predict(enc_ids, enc_pad_mask);
        let (_, pos) = predictions[0];
        // Position should be in non-padded region (0-2)
        assert!(pos < 3, "predicted pos {} should be < 3 (non-pad region)", pos);
    }
}
