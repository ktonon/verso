use burn::nn::attention::generate_autoregressive_mask;
use burn::nn::transformer::{
    TransformerDecoder, TransformerDecoderConfig, TransformerDecoderInput,
    TransformerEncoder, TransformerEncoderConfig, TransformerEncoderInput,
};
use burn::nn::{Embedding, EmbeddingConfig, Linear, LinearConfig};
use burn::prelude::*;

use crate::config::TrainConfig;

/// Transformer encoder-decoder for expression simplification.
///
/// Translates from Python's `SimplificationModel(nn.Module)`.
#[derive(Module, Debug)]
pub struct SimplificationModel<B: Backend> {
    // Encoder
    enc_tok_emb: Embedding<B>,
    enc_pos_emb: Embedding<B>,
    encoder: TransformerEncoder<B>,

    // Decoder
    dec_tok_emb: Embedding<B>,
    dec_pos_emb: Embedding<B>,
    decoder: TransformerDecoder<B>,

    // Output
    output_proj: Linear<B>,
}

impl<B: Backend> SimplificationModel<B> {
    pub fn new(config: &TrainConfig, enc_vocab_size: usize, dec_vocab_size: usize, device: &B::Device) -> Self {
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

        let dec_tok_emb = EmbeddingConfig::new(dec_vocab_size, config.d_model).init(device);
        let dec_pos_emb = EmbeddingConfig::new(config.max_dec_len, config.d_model).init(device);
        let decoder = TransformerDecoderConfig::new(
            config.d_model,
            config.d_ff,
            config.n_heads,
            config.n_decoder_layers,
        )
        .with_dropout(config.dropout)
        .init(device);

        let output_proj = LinearConfig::new(config.d_model, dec_vocab_size).init(device);

        Self {
            enc_tok_emb,
            enc_pos_emb,
            encoder,
            dec_tok_emb,
            dec_pos_emb,
            decoder,
            output_proj,
        }
    }

    /// Forward pass: encoder-decoder with causal masking.
    ///
    /// Args:
    ///   enc_ids: [B, S_enc] encoder token IDs
    ///   dec_input: [B, S_dec] decoder input IDs (BOS + shifted target)
    ///   enc_pad_mask: [B, S_enc] true where padded
    ///   dec_pad_mask: [B, S_dec] true where padded
    ///
    /// Returns:
    ///   logits: [B, S_dec, dec_vocab_size]
    pub fn forward(
        &self,
        enc_ids: Tensor<B, 2, Int>,
        dec_input: Tensor<B, 2, Int>,
        enc_pad_mask: Tensor<B, 2, Bool>,
        dec_pad_mask: Tensor<B, 2, Bool>,
    ) -> Tensor<B, 3> {
        let device = &enc_ids.device();
        let batch_size = enc_ids.dims()[0];

        // Encoder
        let enc_len = enc_ids.dims()[1];
        let enc_pos = Tensor::<B, 1, Int>::arange(0..enc_len as i64, device)
            .unsqueeze_dim::<2>(0)
            .expand([batch_size, enc_len]);
        let enc_emb = self.enc_tok_emb.forward(enc_ids) + self.enc_pos_emb.forward(enc_pos);
        let memory = self
            .encoder
            .forward(TransformerEncoderInput::new(enc_emb).mask_pad(enc_pad_mask.clone()));

        // Decoder
        let dec_len = dec_input.dims()[1];
        let dec_pos = Tensor::<B, 1, Int>::arange(0..dec_len as i64, device)
            .unsqueeze_dim::<2>(0)
            .expand([batch_size, dec_len]);
        let dec_emb = self.dec_tok_emb.forward(dec_input) + self.dec_pos_emb.forward(dec_pos);
        let causal_mask = generate_autoregressive_mask::<B>(batch_size, dec_len, device);
        let decoder_output = self.decoder.forward(
            TransformerDecoderInput::new(dec_emb, memory)
                .target_mask_pad(dec_pad_mask)
                .target_mask_attn(causal_mask)
                .memory_mask_pad(enc_pad_mask),
        );

        self.output_proj.forward(decoder_output)
    }

    /// Greedy autoregressive generation (argmax at each step).
    ///
    /// Returns a list of token ID sequences (one per batch element), excluding BOS.
    pub fn generate(
        &self,
        enc_ids: Tensor<B, 2, Int>,
        max_len: usize,
        enc_pad_mask: Option<Tensor<B, 2, Bool>>,
        stop_token: i64,
    ) -> Vec<Vec<i64>> {
        let device = &enc_ids.device();
        let batch_size = enc_ids.dims()[0];
        let enc_len = enc_ids.dims()[1];

        // Encode once
        let enc_pos = Tensor::<B, 1, Int>::arange(0..enc_len as i64, device)
            .unsqueeze_dim::<2>(0)
            .expand([batch_size, enc_len]);
        let enc_emb = self.enc_tok_emb.forward(enc_ids) + self.enc_pos_emb.forward(enc_pos);
        let encoder_input = if let Some(ref mask) = enc_pad_mask {
            TransformerEncoderInput::new(enc_emb).mask_pad(mask.clone())
        } else {
            TransformerEncoderInput::new(enc_emb)
        };
        let memory = self.encoder.forward(encoder_input);

        // Start with BOS
        let mut generated: Vec<Vec<i64>> = vec![vec![1]; batch_size]; // BOS=1
        let mut finished = vec![false; batch_size];

        for _ in 0..max_len {
            let dec_len = generated[0].len();

            // Build decoder input tensor from generated sequences
            let flat: Vec<i64> = generated.iter().flat_map(|s| s.iter().copied()).collect();
            let dec_ids = Tensor::<B, 2, Int>::from_data(
                TensorData::new(flat, [batch_size, dec_len]),
                device,
            );

            let dec_pos = Tensor::<B, 1, Int>::arange(0..dec_len as i64, device)
                .unsqueeze_dim::<2>(0)
                .expand([batch_size, dec_len]);
            let dec_emb =
                self.dec_tok_emb.forward(dec_ids) + self.dec_pos_emb.forward(dec_pos);

            let causal_mask = generate_autoregressive_mask::<B>(batch_size, dec_len, device);
            let mut decoder_input =
                TransformerDecoderInput::new(dec_emb, memory.clone()).target_mask_attn(causal_mask);
            if let Some(ref mask) = enc_pad_mask {
                decoder_input = decoder_input.memory_mask_pad(mask.clone());
            }
            let output = self.decoder.forward(decoder_input);

            // Get logits for the last position: project all positions then select last
            let all_logits = self.output_proj.forward(output); // [B, S, V]
            let last_logits = all_logits.slice([0..batch_size, (dec_len - 1)..dec_len]); // [B, 1, V]
            let vocab_size = last_logits.dims()[2];
            let logits = last_logits.reshape([batch_size, vocab_size]); // [B, V]

            // Argmax
            let next_tokens = logits.argmax(1); // [B, 1]
            let next_data: Vec<i64> = next_tokens.reshape([batch_size]).into_data().to_vec().unwrap();

            for (i, &tok) in next_data.iter().enumerate() {
                if !finished[i] {
                    generated[i].push(tok);
                    if tok == stop_token {
                        finished[i] = true;
                    }
                } else {
                    // Keep all sequences the same length for tensor construction
                    generated[i].push(stop_token);
                }
            }

            if finished.iter().all(|&f| f) {
                break;
            }
        }

        // Strip BOS, truncate at STOP
        generated
            .into_iter()
            .map(|seq| {
                let seq = &seq[1..]; // skip BOS
                if let Some(pos) = seq.iter().position(|&t| t == stop_token) {
                    seq[..=pos].to_vec()
                } else {
                    seq.to_vec()
                }
            })
            .collect()
    }

    /// Sample action sequences for REINFORCE rollouts.
    ///
    /// Like generate() but samples from the softmax distribution instead of argmax.
    pub fn sample(
        &self,
        enc_ids: Tensor<B, 2, Int>,
        max_len: usize,
        enc_pad_mask: Option<Tensor<B, 2, Bool>>,
        stop_token: i64,
        temperature: f64,
    ) -> Vec<Vec<i64>> {
        let device = &enc_ids.device();
        let batch_size = enc_ids.dims()[0];
        let enc_len = enc_ids.dims()[1];

        // Encode once
        let enc_pos = Tensor::<B, 1, Int>::arange(0..enc_len as i64, device)
            .unsqueeze_dim::<2>(0)
            .expand([batch_size, enc_len]);
        let enc_emb = self.enc_tok_emb.forward(enc_ids) + self.enc_pos_emb.forward(enc_pos);
        let encoder_input = if let Some(ref mask) = enc_pad_mask {
            TransformerEncoderInput::new(enc_emb).mask_pad(mask.clone())
        } else {
            TransformerEncoderInput::new(enc_emb)
        };
        let memory = self.encoder.forward(encoder_input);

        // Start with BOS
        let mut generated: Vec<Vec<i64>> = vec![vec![1]; batch_size]; // BOS=1
        let mut finished = vec![false; batch_size];

        for _ in 0..max_len {
            let dec_len = generated[0].len();

            let flat: Vec<i64> = generated.iter().flat_map(|s| s.iter().copied()).collect();
            let dec_ids = Tensor::<B, 2, Int>::from_data(
                TensorData::new(flat, [batch_size, dec_len]),
                device,
            );

            let dec_pos = Tensor::<B, 1, Int>::arange(0..dec_len as i64, device)
                .unsqueeze_dim::<2>(0)
                .expand([batch_size, dec_len]);
            let dec_emb =
                self.dec_tok_emb.forward(dec_ids) + self.dec_pos_emb.forward(dec_pos);

            let causal_mask = generate_autoregressive_mask::<B>(batch_size, dec_len, device);
            let mut decoder_input =
                TransformerDecoderInput::new(dec_emb, memory.clone()).target_mask_attn(causal_mask);
            if let Some(ref mask) = enc_pad_mask {
                decoder_input = decoder_input.memory_mask_pad(mask.clone());
            }
            let output = self.decoder.forward(decoder_input);

            let all_logits = self.output_proj.forward(output); // [B, S, V]
            let last_logits = all_logits.slice([0..batch_size, (dec_len - 1)..dec_len]); // [B, 1, V]
            let vocab_size = last_logits.dims()[2];
            let logits = last_logits.reshape([batch_size, vocab_size]); // [B, V]

            // Clamp to prevent inf/nan after RL updates
            let logits = logits.clamp(-50.0, 50.0);

            // Softmax with temperature, then multinomial sampling
            let scaled = logits / temperature;
            let probs = burn::tensor::activation::softmax(scaled, 1); // [B, V]
            let next_tokens = multinomial_sample(probs, device);

            for (i, &tok) in next_tokens.iter().enumerate() {
                if !finished[i] {
                    generated[i].push(tok);
                    if tok == stop_token {
                        finished[i] = true;
                    }
                } else {
                    generated[i].push(stop_token);
                }
            }

            if finished.iter().all(|&f| f) {
                break;
            }
        }

        // Strip BOS, truncate at STOP
        generated
            .into_iter()
            .map(|seq| {
                let seq = &seq[1..]; // skip BOS
                if let Some(pos) = seq.iter().position(|&t| t == stop_token) {
                    seq[..=pos].to_vec()
                } else {
                    seq.to_vec()
                }
            })
            .collect()
    }
}

/// Multinomial sampling from probability distributions.
///
/// For each row in `probs` [B, V], samples one index proportional to the probabilities.
/// Implements via cumulative sum + uniform random + searchsorted.
fn multinomial_sample<B: Backend>(probs: Tensor<B, 2>, device: &B::Device) -> Vec<i64> {
    let batch_size = probs.dims()[0];
    let vocab_size = probs.dims()[1];

    // Cumulative sum along vocab dimension
    let cumsum = probs.cumsum(1); // [B, V]

    // Sample uniform random values [B, 1]
    let uniform = Tensor::<B, 2>::random([batch_size, 1], burn::tensor::Distribution::Uniform(0.0, 1.0), device);

    // For each batch element, find the first index where cumsum >= uniform
    // This is equivalent to torch.multinomial(probs, 1)
    let ge = cumsum.greater_equal(uniform); // [B, V] bool

    // Convert bool to float, then argmax to find first True
    // We want the first True, so we use (1 - float(ge)).argmin or use a trick:
    // Multiply indices by mask, set unmasked to large value, take argmin
    let indices = Tensor::<B, 1, Int>::arange(0..vocab_size as i64, device)
        .float()
        .unsqueeze_dim::<2>(0)
        .expand([batch_size, vocab_size]); // [B, V]
    let large = Tensor::<B, 2>::full([batch_size, vocab_size], vocab_size as f32, device);
    let masked_indices = ge.clone().float().mul(indices) + ge.bool_not().float().mul(large);
    let sampled = masked_indices.argmin(1); // [B, 1]

    sampled.reshape([batch_size]).into_data().to_vec().unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use burn::backend::NdArray;
    use crate::config::TrainConfig;
    use crate::dataset::{SimplificationBatcher, SimplificationBatch, SimplificationDataset};
    use crate::vocab::{DecoderVocab, EncoderVocab};
    use burn::data::dataloader::batcher::Batcher;
    use burn::data::dataset::Dataset;
    use clap::Parser;
    use erd_symbolic::random_search::IndexedRuleSet;
    use erd_symbolic::RuleSet;

    type TestBackend = NdArray;

    fn default_config() -> TrainConfig {
        TrainConfig::parse_from::<Vec<&str>, &str>(vec![])
    }

    fn make_model() -> (SimplificationModel<TestBackend>, usize, usize) {
        let config = default_config();
        let indexed = IndexedRuleSet::new(RuleSet::full());
        let enc_vocab = EncoderVocab::new(&indexed);
        let dec_vocab = DecoderVocab::new(&indexed, 64);
        let device = burn::backend::ndarray::NdArrayDevice::Cpu;
        let model = SimplificationModel::new(
            &config,
            enc_vocab.size(),
            dec_vocab.size(),
            &device,
        );
        (model, enc_vocab.size(), dec_vocab.size())
    }

    #[test]
    fn test_parameter_count() {
        let (model, _, _) = make_model();
        let num_params = model.num_params();
        println!("Parameter count: {}", num_params);
        // Python model has ~1,430,549 params
        // Allow 10% difference due to different layer norm implementations
        let lower = 1_200_000;
        let upper = 1_700_000;
        assert!(
            num_params > lower && num_params < upper,
            "param count {} outside expected range [{}, {}]",
            num_params,
            lower,
            upper
        );
    }

    #[test]
    fn test_forward_shapes() {
        let (model, _, dec_vocab_size) = make_model();
        let device = burn::backend::ndarray::NdArrayDevice::Cpu;

        let batch_size = 2;
        let enc_len = 5;
        let dec_len = 7;

        let enc_ids = Tensor::<TestBackend, 2, Int>::from_data(
            TensorData::new(vec![1i64; batch_size * enc_len], [batch_size, enc_len]),
            &device,
        );
        let dec_input = Tensor::<TestBackend, 2, Int>::from_data(
            TensorData::new(vec![1i64; batch_size * dec_len], [batch_size, dec_len]),
            &device,
        );
        let enc_pad_mask = Tensor::<TestBackend, 2, Bool>::from_data(
            TensorData::from([[false; 5]; 2]),
            &device,
        );
        let dec_pad_mask = Tensor::<TestBackend, 2, Bool>::from_data(
            TensorData::from([[false; 7]; 2]),
            &device,
        );

        let logits = model.forward(enc_ids, dec_input, enc_pad_mask, dec_pad_mask);
        let shape = logits.shape();
        assert_eq!(shape.dims[0], batch_size);
        assert_eq!(shape.dims[1], dec_len);
        assert_eq!(shape.dims[2], dec_vocab_size);
        println!("Logits shape: {:?}", shape);
    }

    #[test]
    fn test_generate_produces_tokens() {
        let (model, _, _) = make_model();
        let device = burn::backend::ndarray::NdArrayDevice::Cpu;

        let enc_ids = Tensor::<TestBackend, 2, Int>::from_data(
            TensorData::new(vec![1i64, 2, 3, 4, 5], [1, 5]),
            &device,
        );

        let results = model.generate(enc_ids, 20, None, DecoderVocab::STOP as i64);
        assert_eq!(results.len(), 1);
        // Should produce at least one token
        assert!(!results[0].is_empty(), "generate should produce at least one token");
        println!("Generated {} tokens: {:?}", results[0].len(), results[0]);
    }

    #[test]
    fn test_sample_produces_tokens() {
        let (model, _, _) = make_model();
        let device = burn::backend::ndarray::NdArrayDevice::Cpu;

        let enc_ids = Tensor::<TestBackend, 2, Int>::from_data(
            TensorData::new(vec![1i64, 2, 3, 4, 5], [1, 5]),
            &device,
        );

        let results = model.sample(enc_ids, 20, None, DecoderVocab::STOP as i64, 1.0);
        assert_eq!(results.len(), 1);
        assert!(!results[0].is_empty(), "sample should produce at least one token");
        println!("Sampled {} tokens: {:?}", results[0].len(), results[0]);
    }

    #[test]
    fn test_forward_with_real_batch() {
        let indexed = IndexedRuleSet::new(RuleSet::full());
        let enc_vocab = EncoderVocab::new(&indexed);
        let dec_vocab = DecoderVocab::new(&indexed, 64);
        let config = default_config();
        let device = burn::backend::ndarray::NdArrayDevice::Cpu;
        let model = SimplificationModel::new(&config, enc_vocab.size(), dec_vocab.size(), &device);

        // Create synthetic examples
        let examples = vec![
            erd_symbolic::training_data::TrainingExample {
                input_tokens: vec!["ADD".to_string(), "I_1".to_string(), "I_2".to_string()],
                actions: vec![erd_symbolic::training_data::TrainingAction {
                    rule_direction: 2,
                    position: 0,
                }],
                output_complexity: 1,
                input_complexity: 3,
            },
            erd_symbolic::training_data::TrainingExample {
                input_tokens: vec!["MUL".to_string(), "V0".to_string(), "I_3".to_string()],
                actions: vec![
                    erd_symbolic::training_data::TrainingAction {
                        rule_direction: 5,
                        position: 0,
                    },
                    erd_symbolic::training_data::TrainingAction {
                        rule_direction: 8,
                        position: 1,
                    },
                ],
                output_complexity: 2,
                input_complexity: 3,
            },
        ];

        let dataset = SimplificationDataset::new(&examples, &enc_vocab, &dec_vocab, 50);
        let items: Vec<_> = (0..dataset.len())
            .map(|i| dataset.get(i).unwrap())
            .collect();
        let batcher = SimplificationBatcher;
        let batch: SimplificationBatch<TestBackend> = batcher.batch(items, &device);

        let logits = model.forward(batch.enc_ids, batch.dec_input, batch.enc_pad_mask, batch.dec_pad_mask);
        println!("Real batch logits shape: {:?}", logits.shape());
        assert_eq!(logits.shape().dims[0], 2); // batch size
    }
}
