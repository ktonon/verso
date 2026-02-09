use burn::optim::grad_clipping::GradientClippingConfig;
use burn::optim::{AdamWConfig, GradientsParams, Optimizer};
use burn::prelude::*;
use burn::tensor::backend::AutodiffBackend;

use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::SeedableRng;

use erd_symbolic::random_search::IndexedRuleSet;
use erd_symbolic::token::tokenize;
use erd_symbolic::training_data::{token_to_string, TrainingExample};
use erd_symbolic::validate::{validate_action_sequence, PredictedAction};
use erd_symbolic::RuleSet;

use crate::config::PolicyConfig;
use crate::dataset::load_data;
use crate::evaluate::{compute_metrics, print_metrics, tokens_to_expr};
use crate::policy_evaluate::policy_inference_loop;
use crate::policy_model::PolicyModel;
use crate::policy_train::{load_policy_model, save_policy_checkpoint};
use crate::vocab::EncoderVocab;

/// CLI configuration for policy RL training.
#[derive(clap::Parser, Debug, Clone)]
pub struct PolicyRLConfig {
    #[arg(long, default_value = "checkpoints/policy_best")]
    pub checkpoint: String,
    #[arg(long, default_value = "data_training")]
    pub data_dir: String,

    // RL hyperparameters
    #[arg(long, default_value_t = 32)]
    pub batch_size: usize,
    #[arg(long, default_value_t = 1e-5)]
    pub lr: f64,
    #[arg(long, default_value_t = 1.0)]
    pub temperature: f64,
    #[arg(long, default_value_t = 50)]
    pub max_epochs: usize,
    #[arg(long, default_value_t = 5)]
    pub eval_every: usize,
    #[arg(long, default_value_t = 0.99)]
    pub baseline_decay: f64,
    #[arg(long, default_value_t = 0.05)]
    pub entropy_bonus: f64,
    #[arg(long, default_value_t = 0.5)]
    pub invalid_penalty: f64,
    #[arg(long, default_value_t = 20)]
    pub max_eval_steps: usize,

    // Infrastructure
    #[arg(long, default_value = "checkpoints")]
    pub checkpoint_dir: String,
    #[arg(long, default_value = "cpu")]
    pub device: String,
    #[arg(long, default_value_t = 10)]
    pub log_every: usize,

    // Data
    #[arg(long, default_value_t = 0.1)]
    pub val_fraction: f64,
    #[arg(long, default_value_t = 42)]
    pub seed: u64,

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

impl PolicyRLConfig {
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

/// Encode a single expression into tensors for the policy model.
fn encode_single_expr<B: Backend>(
    expr: &erd_symbolic::Expr,
    enc_vocab: &EncoderVocab,
    device: &B::Device,
) -> (Tensor<B, 2, Int>, Tensor<B, 2, Bool>) {
    let (tokens, _db) = tokenize(expr);
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

    (enc_ids, enc_pad_mask)
}

/// Single-step REINFORCE: sample one action per expression, compute reward, update.
///
/// Returns (updated_model, optimizer, loss, mean_reward, new_baseline, num_valid).
#[allow(clippy::too_many_arguments)]
fn policy_rl_step<B: AutodiffBackend, O: Optimizer<PolicyModel<B>, B>>(
    model: PolicyModel<B>,
    batch: &[TrainingExample],
    enc_vocab: &EncoderVocab,
    rules: &IndexedRuleSet,
    mut optimizer: O,
    config: &PolicyRLConfig,
    baseline: f64,
    device: &B::Device,
) -> (PolicyModel<B>, O, f64, f64, f64, usize) {
    let batch_size = batch.len();

    // 1. For each example: parse, tokenize, sample action, compute reward
    let mut rewards = Vec::with_capacity(batch_size);
    let mut rule_log_probs_vec = Vec::with_capacity(batch_size);
    let mut pos_log_probs_vec = Vec::with_capacity(batch_size);
    let mut num_valid = 0usize;

    // Collect sampled actions and log probs (detached — sampling doesn't track grad)
    let mut sampled_rules = Vec::with_capacity(batch_size);
    let mut sampled_positions = Vec::with_capacity(batch_size);
    let mut enc_data: Vec<(Vec<i64>, usize)> = Vec::with_capacity(batch_size); // (ids, len)

    for ex in batch {
        let expr = match tokens_to_expr(&ex.input_tokens) {
            Ok(e) => e,
            Err(_) => {
                rewards.push(-config.invalid_penalty);
                rule_log_probs_vec.push(0.0f64);
                pos_log_probs_vec.push(0.0f64);
                sampled_rules.push(0usize);
                sampled_positions.push(0usize);
                enc_data.push((vec![1], 1)); // dummy token
                continue;
            }
        };

        let (enc_ids, enc_pad_mask) = encode_single_expr::<B>(&expr, enc_vocab, device);
        let seq_len = enc_ids.dims()[1];

        // Sample one action
        let samples = model.sample(enc_ids, enc_pad_mask, config.temperature);
        let (rule, pos, log_p_rule, log_p_pos) = samples[0];

        // Store for later gradient computation
        let (tokens, _db) = tokenize(&expr);
        let ids: Vec<i64> = tokens
            .iter()
            .map(|t| enc_vocab.encode(&token_to_string(t)) as i64)
            .collect();
        enc_data.push((ids, seq_len));
        sampled_rules.push(rule);
        sampled_positions.push(pos);
        rule_log_probs_vec.push(log_p_rule);
        pos_log_probs_vec.push(log_p_pos);

        // Validate action
        let predicted = PredictedAction {
            rule_direction: rule as u16,
            position: pos,
        };
        let result = validate_action_sequence(&expr, &[predicted], rules);

        if result.valid_steps == 1 {
            num_valid += 1;
            let delta = result.input_complexity as f64 - result.final_complexity as f64;
            rewards.push(delta);
        } else {
            rewards.push(-config.invalid_penalty);
        }
    }

    let mean_reward: f64 = rewards.iter().sum::<f64>() / batch_size as f64;

    // 2. Re-encode all expressions in a single padded batch for gradient computation
    let max_len = enc_data.iter().map(|(_, l)| *l).max().unwrap_or(1);
    let flat: Vec<i64> = enc_data
        .iter()
        .flat_map(|(ids, _)| {
            let mut padded = ids.clone();
            padded.resize(max_len, 0);
            padded
        })
        .collect();

    let enc_ids = Tensor::<B, 2, Int>::from_data(
        TensorData::new(flat, [batch_size, max_len]),
        device,
    );
    let enc_pad_mask = enc_ids.clone().equal_elem(0);

    // 3. Forward pass with gradient tracking
    let (rule_logits, pos_logits) = model.forward(enc_ids, enc_pad_mask);
    // rule_logits: [B, R], pos_logits: [B, S]

    // 4. Compute log probs at sampled indices
    let rule_log_sm = burn::tensor::activation::log_softmax(rule_logits.clone(), 1);
    let pos_log_sm = burn::tensor::activation::log_softmax(pos_logits.clone(), 1);

    // Gather rule log probs
    let rule_indices = Tensor::<B, 2, Int>::from_data(
        TensorData::new(
            sampled_rules.iter().map(|&r| r as i64).collect::<Vec<_>>(),
            [batch_size, 1],
        ),
        device,
    );
    let rule_lp: Tensor<B, 1> = rule_log_sm.clone().gather(1, rule_indices).reshape([batch_size]);

    // Gather position log probs
    let pos_indices = Tensor::<B, 2, Int>::from_data(
        TensorData::new(
            sampled_positions.iter().map(|&p| p as i64).collect::<Vec<_>>(),
            [batch_size, 1],
        ),
        device,
    );
    let pos_lp: Tensor<B, 1> = pos_log_sm.clone().gather(1, pos_indices).reshape([batch_size]);

    let total_log_prob = rule_lp + pos_lp; // [B]

    // 5. Compute advantages
    let reward_f32: Vec<f32> = rewards.iter().map(|&r| r as f32).collect();
    let reward_tensor =
        Tensor::<B, 1>::from_data(TensorData::new(reward_f32, [batch_size]), device);
    let baseline_tensor = Tensor::<B, 1>::full([batch_size], baseline as f32, device);
    let mut advantages = reward_tensor - baseline_tensor;

    if batch_size > 1 {
        let std_val = advantages
            .clone()
            .powf_scalar(2.0)
            .mean()
            .sqrt()
            .into_data()
            .to_vec::<f32>()
            .unwrap()[0];
        if std_val > 1e-8 {
            advantages = advantages / (std_val + 1e-8);
        }
    }

    // 6. REINFORCE loss: -(log_prob * advantage).mean()
    let rl_loss = (total_log_prob * advantages).mean().neg();

    // 7. Entropy bonus: H = -sum(p * log_p) for each head, averaged over batch
    let rule_probs = burn::tensor::activation::softmax(rule_logits, 1);
    let rule_entropy: Tensor<B, 1> =
        (rule_probs * rule_log_sm).sum_dim(1).neg().reshape([batch_size]);

    let pos_probs = burn::tensor::activation::softmax(pos_logits, 1);
    let pos_entropy: Tensor<B, 1> =
        (pos_probs * pos_log_sm).sum_dim(1).neg().reshape([batch_size]);

    let mean_entropy = (rule_entropy + pos_entropy).mean();
    let loss = rl_loss - mean_entropy * config.entropy_bonus;

    // 8. Backward + update
    let loss_value = loss.clone().into_data().to_vec::<f32>().unwrap()[0] as f64;
    let grads = loss.backward();
    let grads = GradientsParams::from_grads(grads, &model);
    let model = optimizer.step(config.lr, model, grads);

    // 9. Update baseline (EMA)
    let new_baseline =
        config.baseline_decay * baseline + (1.0 - config.baseline_decay) * mean_reward;

    (model, optimizer, loss_value, mean_reward, new_baseline, num_valid)
}

/// Full REINFORCE training loop for the policy model.
pub fn policy_rl_train<B: AutodiffBackend>(config: PolicyRLConfig, device: B::Device) {
    let indexed = IndexedRuleSet::new(RuleSet::full());
    let enc_vocab = EncoderVocab::new(&indexed);
    let num_rules = indexed.total_directions as usize;

    println!("Loading data from {}...", config.data_dir);
    let (mut train_examples, val_examples) =
        load_data(&config.data_dir, config.val_fraction, config.seed);
    println!(
        "  Train: {}, Val: {}",
        train_examples.len(),
        val_examples.len()
    );

    // Load model
    let policy_config = config.to_policy_config();
    let rl_best_path = format!("{}/policy_rl_best", config.checkpoint_dir);
    let rl_meta_path = format!("{}/policy_rl_best_metadata.json", config.checkpoint_dir);

    let (model, start_epoch, mut baseline, mut best_eval_reward) =
        if std::path::Path::new(&format!("{}.mpk", rl_best_path)).exists() {
            println!("Resuming from RL checkpoint: {}", rl_best_path);
            let model: PolicyModel<B> = load_policy_model(
                &policy_config,
                enc_vocab.size(),
                num_rules,
                &device,
                &rl_best_path,
            );
            let (epoch, bl, best_reward) =
                if let Ok(data) = std::fs::read_to_string(&rl_meta_path) {
                    let meta: serde_json::Value = serde_json::from_str(&data).unwrap();
                    let epoch = meta["epoch"].as_u64().unwrap_or(0) as usize;
                    let bl = meta.get("baseline").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let best_reward = meta
                        .get("best_eval_reward")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(f64::NEG_INFINITY);
                    (epoch, bl, best_reward)
                } else {
                    (0, 0.0, f64::NEG_INFINITY)
                };
            println!(
                "  Resumed at epoch {}, baseline={:.3}, best_reward={:+.3}",
                epoch + 1, bl, best_reward
            );
            (model, epoch + 1, bl, best_reward)
        } else {
            println!("Loading supervised checkpoint: {}", config.checkpoint);
            let model: PolicyModel<B> = load_policy_model(
                &policy_config,
                enc_vocab.size(),
                num_rules,
                &device,
                &config.checkpoint,
            );
            (model, 0, 0.0f64, f64::NEG_INFINITY)
        };
    println!("Model parameters: {}", model.num_params());

    // Optimizer
    let optimizer = AdamWConfig::new()
        .with_weight_decay(0.01_f32)
        .with_grad_clipping(Some(GradientClippingConfig::Norm(1.0)))
        .init::<B, PolicyModel<B>>();

    let mut rng = StdRng::seed_from_u64(config.seed);

    println!(
        "\nPolicy RL training for up to {} epochs...",
        config.max_epochs
    );
    println!(
        "  batch_size={}, lr={}, temperature={}",
        config.batch_size, config.lr, config.temperature
    );

    let mut model = model;
    let mut optimizer = optimizer;

    for epoch in start_epoch..config.max_epochs {
        let t0 = std::time::Instant::now();
        train_examples.shuffle(&mut rng);

        let mut epoch_loss = 0.0f64;
        let mut epoch_reward = 0.0f64;
        let mut epoch_valid = 0usize;
        let mut num_batches = 0usize;

        for start in (0..train_examples.len()).step_by(config.batch_size) {
            let end = (start + config.batch_size).min(train_examples.len());
            let batch = &train_examples[start..end];
            if batch.len() < 2 {
                continue;
            }

            let (new_model, new_optimizer, loss, mean_reward, new_baseline, n_valid) =
                policy_rl_step(
                    model, batch, &enc_vocab, &indexed, optimizer, &config, baseline, &device,
                );
            model = new_model;
            optimizer = new_optimizer;
            baseline = new_baseline;

            epoch_loss += loss;
            epoch_reward += mean_reward;
            epoch_valid += n_valid;
            num_batches += 1;

            if config.log_every > 0 && num_batches % config.log_every == 0 {
                let avg_loss = epoch_loss / num_batches as f64;
                let avg_reward = epoch_reward / num_batches as f64;
                println!(
                    "  epoch {} batch {}: loss={:.4} reward={:+.3} baseline={:.3}",
                    epoch + 1, num_batches, avg_loss, avg_reward, baseline
                );
            }
        }

        let elapsed = t0.elapsed();
        let avg_loss = epoch_loss / num_batches.max(1) as f64;
        let avg_reward = epoch_reward / num_batches.max(1) as f64;
        let total_examples = num_batches * config.batch_size;
        let validity_rate = epoch_valid as f64 / total_examples.max(1) as f64;

        println!(
            "Epoch {}: loss={:.4}, reward={:+.3}, validity={:.1}%, baseline={:.3}, time={:.1}s",
            epoch + 1,
            avg_loss,
            avg_reward,
            validity_rate * 100.0,
            baseline,
            elapsed.as_secs_f64()
        );

        // Periodic evaluation using the inference loop
        if (epoch + 1) % config.eval_every == 0 {
            println!("\n--- Full evaluation at epoch {} ---", epoch + 1);
            let mut results = Vec::new();
            for ex in val_examples.iter().take(200) {
                if let Ok(expr) = tokens_to_expr(&ex.input_tokens) {
                    let (result, _trace) = policy_inference_loop(
                        &model,
                        &expr,
                        &enc_vocab,
                        &indexed,
                        config.max_eval_steps,
                        &device,
                    );
                    results.push(result);
                }
            }
            let metrics = compute_metrics(&results, config.invalid_penalty);
            print_metrics(&metrics);

            if metrics.mean_reward > best_eval_reward {
                best_eval_reward = metrics.mean_reward;
                save_policy_checkpoint(
                    &model,
                    epoch,
                    best_eval_reward,
                    &config.checkpoint_dir,
                    "policy_rl_best",
                );
            }
            println!();
        }
    }

    // Save final checkpoint
    save_policy_checkpoint(
        &model,
        config.max_epochs - 1,
        best_eval_reward,
        &config.checkpoint_dir,
        "policy_rl_latest",
    );

    println!("\nDone. Best eval reward: {:+.4}", best_eval_reward);
}
