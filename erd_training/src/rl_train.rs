use burn::optim::grad_clipping::GradientClippingConfig;
use burn::optim::{AdamWConfig, GradientsParams, Optimizer};
use burn::prelude::*;
use burn::tensor::backend::AutodiffBackend;

use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::SeedableRng;

use erd_symbolic::random_search::IndexedRuleSet;
use erd_symbolic::training_data::TrainingExample;
use erd_symbolic::validate::validate_action_sequence;
use erd_symbolic::RuleSet;

use crate::config::RLConfig;
use crate::dataset::load_data;
use crate::evaluate::{
    compute_reward, decode_action_sequence, evaluate, print_metrics, tokens_to_expr,
};
use crate::model::SimplificationModel;
use crate::train::{load_model, save_checkpoint_named};
use crate::vocab::{DecoderVocab, EncoderVocab};

/// Encode a batch of expressions into padded tensors.
pub fn encode_expressions<B: Backend>(
    examples: &[TrainingExample],
    enc_vocab: &EncoderVocab,
    device: &B::Device,
) -> (Tensor<B, 2, Int>, Tensor<B, 2, Bool>) {
    let enc_id_seqs: Vec<Vec<i64>> = examples
        .iter()
        .map(|ex| {
            ex.input_tokens
                .iter()
                .map(|t| enc_vocab.encode(t) as i64)
                .collect()
        })
        .collect();

    let max_len = enc_id_seqs.iter().map(|s| s.len()).max().unwrap_or(0);
    let batch_size = examples.len();

    let flat: Vec<i64> = enc_id_seqs
        .iter()
        .flat_map(|s| {
            let mut padded = s.clone();
            padded.resize(max_len, 0);
            padded
        })
        .collect();

    let enc_ids =
        Tensor::<B, 2, Int>::from_data(TensorData::new(flat, [batch_size, max_len]), device);
    let enc_pad_mask = enc_ids.clone().equal_elem(0);

    (enc_ids, enc_pad_mask)
}

/// One REINFORCE training step.
///
/// Returns (updated_model, loss_value, mean_reward, new_baseline, num_fully_valid).
#[allow(clippy::too_many_arguments)]
pub fn rl_train_step<B: AutodiffBackend, O: Optimizer<SimplificationModel<B>, B>>(
    model: SimplificationModel<B>,
    batch: &[TrainingExample],
    enc_vocab: &EncoderVocab,
    dec_vocab: &DecoderVocab,
    rules: &IndexedRuleSet,
    mut optimizer: O,
    config: &RLConfig,
    baseline: f64,
    device: &B::Device,
) -> (SimplificationModel<B>, O, f64, f64, f64, usize) {
    let batch_size = batch.len();

    // 1. Encode input expressions
    let (enc_ids, enc_pad_mask) = encode_expressions::<B>(batch, enc_vocab, device);

    // 2. Sample action sequences (no grad needed — sample uses detached tensors)
    let sampled_ids = model.sample(
        enc_ids.clone(),
        config.max_gen_len,
        Some(enc_pad_mask.clone()),
        2, // STOP token
        config.temperature,
    );

    // 3. Decode sampled tokens into actions
    let all_actions: Vec<_> = sampled_ids
        .iter()
        .map(|ids| decode_action_sequence(ids, dec_vocab))
        .collect();

    // 4. Validate via direct function call
    let mut rewards = Vec::with_capacity(batch_size);
    let mut num_valid = 0usize;

    for (i, ex) in batch.iter().enumerate() {
        match tokens_to_expr(&ex.input_tokens) {
            Ok(expr) => {
                let result = validate_action_sequence(&expr, &all_actions[i], rules);
                let reward = compute_reward(&result, config.invalid_penalty);
                if result.valid_steps == result.total_steps && result.total_steps > 0 {
                    num_valid += 1;
                }
                rewards.push(reward);
            }
            Err(_) => {
                rewards.push(-(all_actions[i].len() as f64) * config.invalid_penalty);
            }
        }
    }

    let mean_reward: f64 = rewards.iter().sum::<f64>() / batch_size as f64;

    // 5. Build decoder tensors from sampled sequences
    // dec_input = [BOS] + sampled[:-1], dec_target = sampled
    let mut dec_input_seqs: Vec<Vec<i64>> = Vec::with_capacity(batch_size);
    let mut dec_target_seqs: Vec<Vec<i64>> = Vec::with_capacity(batch_size);
    let mut seq_lengths: Vec<f64> = Vec::with_capacity(batch_size);

    for ids in &sampled_ids {
        let target: Vec<i64> = ids.clone();
        let mut input = vec![DecoderVocab::BOS as i64];
        if !target.is_empty() {
            input.extend_from_slice(&target[..target.len() - 1]);
        }
        seq_lengths.push((target.len().max(1)) as f64);
        dec_input_seqs.push(input);
        dec_target_seqs.push(target);
    }

    let max_dec_len = dec_target_seqs.iter().map(|s| s.len()).max().unwrap_or(1);

    let dec_input_flat: Vec<i64> = dec_input_seqs
        .iter()
        .flat_map(|s| {
            let mut p = s.clone();
            p.resize(max_dec_len, 0);
            p
        })
        .collect();
    let dec_target_flat: Vec<i64> = dec_target_seqs
        .iter()
        .flat_map(|s| {
            let mut p = s.clone();
            p.resize(max_dec_len, 0);
            p
        })
        .collect();

    let dec_input = Tensor::<B, 2, Int>::from_data(
        TensorData::new(dec_input_flat, [batch_size, max_dec_len]),
        device,
    );
    let dec_target = Tensor::<B, 2, Int>::from_data(
        TensorData::new(dec_target_flat, [batch_size, max_dec_len]),
        device,
    );
    let dec_pad_mask = dec_input.clone().equal_elem(0);

    // 6. Forward pass with gradients
    let logits = model.forward(enc_ids, dec_input, enc_pad_mask, dec_pad_mask); // [B, S, V]

    // 7. Log softmax and gather log probs for sampled tokens
    let log_probs = burn::tensor::activation::log_softmax(logits.clone(), 2); // [B, S, V]

    // Gather: extract log prob at each sampled token position
    let token_log_probs: Tensor<B, 2> = log_probs.clone()
        .gather(2, dec_target.clone().unsqueeze_dim(2))
        .squeeze(); // [B, S]

    // Mask padding positions
    let mask = dec_target.clone().not_equal_elem(0).float(); // [B, S]
    let token_log_probs = token_log_probs * mask.clone();

    // Sum per trajectory
    let trajectory_log_probs: Tensor<B, 1> = token_log_probs.sum_dim(1).squeeze(); // [B]

    // Normalize by sequence length
    let seq_len_tensor = Tensor::<B, 1>::from_data(
        TensorData::new(
            seq_lengths.iter().map(|&l| l as f32).collect::<Vec<f32>>(),
            [batch_size],
        ),
        device,
    );
    let normalized_log_probs = trajectory_log_probs / seq_len_tensor;

    // 8. Compute advantages (per-batch normalization)
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

    // 9. REINFORCE loss: -(log_prob * advantage).mean()
    let rl_loss = (normalized_log_probs * advantages).mean().neg();

    // 10. Entropy bonus
    let probs = burn::tensor::activation::softmax(logits, 2); // [B, S, V]
    let entropy: Tensor<B, 2> = (probs * log_probs).sum_dim(2).squeeze::<2>().neg(); // [B, S]
    let masked_entropy = entropy * mask.clone();
    let mean_entropy = masked_entropy.sum() / mask.sum();
    let loss = rl_loss - mean_entropy * config.entropy_bonus;

    // 11. Backward + update
    let loss_value = loss.clone().into_data().to_vec::<f32>().unwrap()[0] as f64;
    let grads = loss.backward();
    let grads = GradientsParams::from_grads(grads, &model);
    let model = optimizer.step(config.lr, model, grads);

    // 12. Update baseline (EMA)
    let new_baseline =
        config.baseline_decay * baseline + (1.0 - config.baseline_decay) * mean_reward;

    (model, optimizer, loss_value, mean_reward, new_baseline, num_valid)
}

/// Full REINFORCE training loop.
pub fn rl_train<B: AutodiffBackend>(config: RLConfig, device: B::Device) {
    let indexed = IndexedRuleSet::new(RuleSet::full());
    let enc_vocab = EncoderVocab::new(&indexed);
    let dec_vocab = DecoderVocab::new(&indexed, config.max_positions);

    println!("Loading data from {}...", config.data_dir);
    let (mut train_examples, val_examples) =
        load_data(&config.data_dir, config.val_fraction, config.seed);
    println!(
        "  Train: {}, Val: {}",
        train_examples.len(),
        val_examples.len()
    );
    println!(
        "  Encoder vocab: {}, Decoder vocab: {}",
        enc_vocab.size(),
        dec_vocab.size()
    );

    // Load supervised checkpoint
    println!("Loading model from {}...", config.checkpoint);
    let train_config = config.to_train_config();
    let model: SimplificationModel<B> = load_model(
        &train_config,
        enc_vocab.size(),
        dec_vocab.size(),
        &device,
        &config.checkpoint,
    );
    println!("Model parameters: {}", model.num_params());

    // Optimizer (lower LR than supervised to prevent catastrophic forgetting)
    let optimizer = AdamWConfig::new()
        .with_weight_decay(0.01_f32)
        .with_grad_clipping(Some(GradientClippingConfig::Norm(1.0)))
        .init::<B, SimplificationModel<B>>();

    let mut baseline = 0.0f64;
    let mut best_eval_reward = f64::NEG_INFINITY;
    let mut rng = StdRng::seed_from_u64(config.seed);

    println!(
        "\nRL training for up to {} epochs...",
        config.max_epochs
    );
    println!(
        "  batch_size={}, lr={}, temperature={}",
        config.batch_size, config.lr, config.temperature
    );
    println!(
        "  entropy_bonus={}, invalid_penalty={}",
        config.entropy_bonus, config.invalid_penalty
    );

    let mut model = model;
    let mut optimizer = optimizer;

    for epoch in 0..config.max_epochs {
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
                rl_train_step(
                    model, batch, &enc_vocab, &dec_vocab, &indexed, optimizer, &config,
                    baseline, &device,
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
                    epoch + 1,
                    num_batches,
                    avg_loss,
                    avg_reward,
                    baseline
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

        // Periodic full evaluation
        if (epoch + 1) % config.eval_every == 0 {
            println!("\n--- Full evaluation at epoch {} ---", epoch + 1);
            let metrics = evaluate(
                &model,
                &val_examples,
                &enc_vocab,
                &dec_vocab,
                &indexed,
                config.batch_size * 2,
                config.invalid_penalty,
                &device,
            );
            print_metrics(&metrics);

            if metrics.mean_reward > best_eval_reward {
                best_eval_reward = metrics.mean_reward;
                save_checkpoint_named(&model, epoch, -metrics.mean_reward, &config.checkpoint_dir, "rl_best");
            }
            println!();
        }
    }

    // Save final checkpoint
    save_checkpoint_named(
        &model,
        config.max_epochs - 1,
        -baseline,
        &config.checkpoint_dir,
        "rl_latest",
    );

    println!("\nDone. Best eval reward: {:+.4}", best_eval_reward);
}
