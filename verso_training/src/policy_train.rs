use std::path::PathBuf;
use std::sync::Arc;

use burn::data::dataloader::{DataLoader, DataLoaderBuilder};
use burn::data::dataset::Dataset;
use burn::nn::loss::{CrossEntropyLoss, CrossEntropyLossConfig};
use burn::optim::grad_clipping::GradientClippingConfig;
use burn::optim::{AdamWConfig, GradientsParams, Optimizer};
use burn::prelude::*;
use burn::record::{FullPrecisionSettings, NamedMpkFileRecorder, Recorder};
use burn::tensor::backend::AutodiffBackend;

use crate::config::PolicyConfig;
use crate::dataset::load_data;
use crate::policy_dataset::{PolicyBatch, PolicyBatcher, PolicyDataset};
use crate::policy_model::PolicyModel;
use crate::schedule::cosine_lr;
use crate::vocab::EncoderVocab;

use verso_symbolic::random_search::IndexedRuleSet;
use verso_symbolic::RuleSet;

/// Configuration for policy supervised training (CLI args).
#[derive(clap::Parser, Debug, Clone)]
pub struct PolicyTrainConfig {
    #[arg(long, default_value = "data_training")]
    pub data_dir: String,
    #[arg(long, default_value_t = 0.1)]
    pub val_fraction: f64,
    #[arg(long, default_value_t = 42)]
    pub seed: u64,

    // Model (encoder-only)
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

    // Training
    #[arg(long, default_value_t = 64)]
    pub batch_size: usize,
    #[arg(long, default_value_t = 3e-4)]
    pub lr: f64,
    #[arg(long, default_value_t = 0.01)]
    pub weight_decay: f64,
    #[arg(long, default_value_t = 200)]
    pub warmup_steps: usize,
    #[arg(long, default_value_t = 100)]
    pub max_epochs: usize,
    #[arg(long, default_value_t = 10)]
    pub patience: usize,
    #[arg(long, default_value = "checkpoints")]
    pub checkpoint_dir: String,
    #[arg(long, default_value_t = 50)]
    pub log_every: usize,
    #[arg(long, default_value = "cpu")]
    pub device: String,
}

impl PolicyTrainConfig {
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

#[derive(Debug, PartialEq)]
struct TrainingSchedule {
    batches_per_epoch: usize,
    total_steps: usize,
}

fn training_schedule(num_train_items: usize, batch_size: usize, max_epochs: usize) -> TrainingSchedule {
    let batches_per_epoch = num_train_items.div_ceil(batch_size);
    TrainingSchedule {
        batches_per_epoch,
        total_steps: batches_per_epoch * max_epochs,
    }
}

fn average_epoch_loss(total_loss: f64, num_batches: usize) -> f64 {
    if num_batches > 0 {
        total_loss / num_batches as f64
    } else {
        0.0
    }
}

#[derive(Debug, PartialEq)]
enum ValidationDecision {
    NewBest,
    Continue { epochs_without_improvement: usize },
    StopEarly,
}

fn validation_decision(
    best_val_loss: f64,
    epochs_without_improvement: usize,
    val_loss: f64,
    patience: usize,
) -> ValidationDecision {
    if val_loss < best_val_loss {
        ValidationDecision::NewBest
    } else {
        let epochs_without_improvement = epochs_without_improvement + 1;
        if epochs_without_improvement >= patience {
            ValidationDecision::StopEarly
        } else {
            ValidationDecision::Continue {
                epochs_without_improvement,
            }
        }
    }
}

/// Run supervised training for the policy model.
pub fn policy_supervised_train<B: AutodiffBackend>(config: PolicyTrainConfig, device: B::Device) {
    println!("Loading data from {}...", config.data_dir);
    let indexed = IndexedRuleSet::new(RuleSet::full());
    let enc_vocab = EncoderVocab::new(&indexed);
    let num_rules = indexed.total_directions as usize;

    let (train_examples, val_examples) =
        load_data(&config.data_dir, config.val_fraction, config.seed);
    println!(
        "  Examples: {} train, {} val",
        train_examples.len(),
        val_examples.len()
    );

    println!("Expanding to per-step policy items...");
    let train_ds = PolicyDataset::from_examples(&train_examples, &enc_vocab, &indexed);
    let val_ds = PolicyDataset::from_examples(&val_examples, &enc_vocab, &indexed);
    println!(
        "  Policy items: {} train, {} val",
        train_ds.len(),
        val_ds.len()
    );
    println!(
        "  Encoder vocab: {}, Rule directions: {}",
        enc_vocab.size(),
        num_rules
    );

    let num_train_items = train_ds.len();

    let train_dl = DataLoaderBuilder::new(PolicyBatcher)
        .batch_size(config.batch_size)
        .shuffle(config.seed)
        .build(train_ds);

    let val_dl = DataLoaderBuilder::new(PolicyBatcher)
        .batch_size(config.batch_size)
        .build(val_ds);

    // Model
    let policy_config = config.to_policy_config();
    let mut model: PolicyModel<B> =
        PolicyModel::new(&policy_config, enc_vocab.size(), num_rules, &device);
    println!("Model parameters: {}", model.num_params());

    // Optimizer
    let mut optimizer = AdamWConfig::new()
        .with_weight_decay(config.weight_decay as f32)
        .with_grad_clipping(Some(GradientClippingConfig::Norm(1.0)))
        .init::<B, PolicyModel<B>>();

    // Losses
    let rule_criterion: CrossEntropyLoss<B> = CrossEntropyLossConfig::new().init(&device);
    let pos_criterion: CrossEntropyLoss<B> = CrossEntropyLossConfig::new().init(&device);

    // Schedule
    let schedule = training_schedule(num_train_items, config.batch_size, config.max_epochs);

    let mut best_val_loss = f64::INFINITY;
    let mut epochs_without_improvement = 0;
    let mut global_step = 0;

    println!(
        "\nTraining for up to {} epochs ({} batches/epoch, {} total steps)...",
        config.max_epochs, schedule.batches_per_epoch, schedule.total_steps
    );

    for epoch in 0..config.max_epochs {
        let t0 = std::time::Instant::now();
        let mut epoch_loss = 0.0f64;
        let mut epoch_rule_loss = 0.0f64;
        let mut epoch_pos_loss = 0.0f64;
        let mut num_batches = 0usize;

        for batch in train_dl.iter() {
            let lr = cosine_lr(
                global_step,
                config.warmup_steps,
                schedule.total_steps,
                config.lr,
            );

            let (rule_logits, pos_logits) = model.forward(batch.enc_ids, batch.enc_pad_mask);

            // Rule loss: [B, R] vs [B]
            let rule_loss = rule_criterion.forward(rule_logits, batch.rule_targets);

            // Position loss: [B, S] vs [B]
            let pos_loss = pos_criterion.forward(pos_logits, batch.position_targets);

            let loss = rule_loss.clone() + pos_loss.clone();
            let loss_value = loss.clone().into_data().to_vec::<f32>().unwrap()[0] as f64;
            let rule_loss_val = rule_loss.into_data().to_vec::<f32>().unwrap()[0] as f64;
            let pos_loss_val = pos_loss.into_data().to_vec::<f32>().unwrap()[0] as f64;

            epoch_loss += loss_value;
            epoch_rule_loss += rule_loss_val;
            epoch_pos_loss += pos_loss_val;

            let grads = loss.backward();
            let grads = GradientsParams::from_grads(grads, &model);
            model = optimizer.step(lr, model, grads);

            global_step += 1;
            num_batches += 1;

            if config.log_every > 0 && num_batches % config.log_every == 0 {
                println!(
                    "  epoch {} batch {}/{}: loss={:.4} (rule={:.4} pos={:.4}) lr={:.2e}",
                    epoch + 1,
                    num_batches,
                    schedule.batches_per_epoch,
                    epoch_loss / num_batches as f64,
                    epoch_rule_loss / num_batches as f64,
                    epoch_pos_loss / num_batches as f64,
                    lr
                );
            }
        }

        let avg_train_loss = average_epoch_loss(epoch_loss, num_batches);

        // Validate
        let (val_loss, val_rule, val_pos) =
            policy_validate(&model, &val_dl, &rule_criterion, &pos_criterion);

        let elapsed = t0.elapsed();
        println!(
            "Epoch {}: train_loss={:.4}, val_loss={:.4} (rule={:.4} pos={:.4}), time={:.1}s",
            epoch + 1,
            avg_train_loss,
            val_loss,
            val_rule,
            val_pos,
            elapsed.as_secs_f64()
        );

        match validation_decision(
            best_val_loss,
            epochs_without_improvement,
            val_loss,
            config.patience,
        ) {
            ValidationDecision::NewBest => {
                best_val_loss = val_loss;
                epochs_without_improvement = 0;
                save_policy_checkpoint(
                    &model,
                    epoch,
                    val_loss,
                    &config.checkpoint_dir,
                    "policy_best",
                );
            }
            ValidationDecision::Continue {
                epochs_without_improvement: next_epochs_without_improvement,
            } => {
                epochs_without_improvement = next_epochs_without_improvement;
            }
            ValidationDecision::StopEarly => {
                println!(
                    "Early stopping after {} epochs (no improvement for {})",
                    epoch + 1,
                    config.patience
                );
                break;
            }
        }
    }

    println!("\nDone. Best val_loss: {:.4}", best_val_loss);
}

/// Compute average validation loss for the policy model.
fn policy_validate<B: AutodiffBackend>(
    model: &PolicyModel<B>,
    val_dl: &Arc<dyn DataLoader<B, PolicyBatch<B>>>,
    rule_criterion: &CrossEntropyLoss<B>,
    pos_criterion: &CrossEntropyLoss<B>,
) -> (f64, f64, f64) {
    let mut total_loss = 0.0f64;
    let mut total_rule = 0.0f64;
    let mut total_pos = 0.0f64;
    let mut num_batches = 0usize;

    for batch in val_dl.iter() {
        let (rule_logits, pos_logits) = model.forward(batch.enc_ids, batch.enc_pad_mask);

        let rule_loss = rule_criterion.forward(rule_logits, batch.rule_targets);
        let pos_loss = pos_criterion.forward(pos_logits, batch.position_targets);
        let loss = rule_loss.clone() + pos_loss.clone();

        total_loss += loss.into_data().to_vec::<f32>().unwrap()[0] as f64;
        total_rule += rule_loss.into_data().to_vec::<f32>().unwrap()[0] as f64;
        total_pos += pos_loss.into_data().to_vec::<f32>().unwrap()[0] as f64;
        num_batches += 1;
    }

    if num_batches > 0 {
        (
            total_loss / num_batches as f64,
            total_rule / num_batches as f64,
            total_pos / num_batches as f64,
        )
    } else {
        (0.0, 0.0, 0.0)
    }
}

fn checkpoint_record_path(checkpoint_dir: &str, name: &str) -> PathBuf {
    PathBuf::from(checkpoint_dir).join(name)
}

fn checkpoint_metadata_path(checkpoint_dir: &str, name: &str) -> PathBuf {
    PathBuf::from(checkpoint_dir).join(format!("{}_metadata.json", name))
}

fn checkpoint_metadata(epoch: usize, val_loss: f64) -> serde_json::Value {
    serde_json::json!({
        "epoch": epoch,
        "val_loss": val_loss,
        "model_type": "policy",
    })
}

/// Save policy model checkpoint.
pub fn save_policy_checkpoint<B: AutodiffBackend>(
    model: &PolicyModel<B>,
    epoch: usize,
    val_loss: f64,
    checkpoint_dir: &str,
    name: &str,
) {
    std::fs::create_dir_all(checkpoint_dir).unwrap();

    let recorder = NamedMpkFileRecorder::<FullPrecisionSettings>::default();
    let path = checkpoint_record_path(checkpoint_dir, name);
    recorder
        .record(model.clone().into_record(), path)
        .expect("Failed to save policy checkpoint");

    let metadata = checkpoint_metadata(epoch, val_loss);
    let meta_path = checkpoint_metadata_path(checkpoint_dir, name);
    std::fs::write(meta_path, serde_json::to_string_pretty(&metadata).unwrap()).unwrap();

    println!("  Checkpoint saved: {}/{}.mpk", checkpoint_dir, name);
}

/// Load a policy model from checkpoint.
pub fn load_policy_model<B: Backend>(
    config: &PolicyConfig,
    enc_vocab_size: usize,
    num_rules: usize,
    device: &B::Device,
    checkpoint_path: &str,
) -> PolicyModel<B> {
    let recorder = NamedMpkFileRecorder::<FullPrecisionSettings>::default();
    let record = recorder
        .load(PathBuf::from(checkpoint_path), device)
        .expect("Failed to load policy checkpoint");
    let model = PolicyModel::new(config, enc_vocab_size, num_rules, device);
    model.load_record(record)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_policy_train_config() -> PolicyTrainConfig {
        PolicyTrainConfig {
            data_dir: "data".to_string(),
            val_fraction: 0.2,
            seed: 7,
            d_model: 192,
            n_encoder_layers: 6,
            n_heads: 8,
            d_ff: 384,
            dropout: 0.25,
            max_enc_len: 96,
            batch_size: 32,
            lr: 1e-4,
            weight_decay: 0.02,
            warmup_steps: 50,
            max_epochs: 20,
            patience: 4,
            checkpoint_dir: "checkpoints".to_string(),
            log_every: 5,
            device: "cpu".to_string(),
        }
    }

    #[test]
    fn policy_train_config_maps_model_fields() {
        let config = sample_policy_train_config();

        let policy = config.to_policy_config();

        assert_eq!(policy.d_model, 192);
        assert_eq!(policy.n_encoder_layers, 6);
        assert_eq!(policy.n_heads, 8);
        assert_eq!(policy.d_ff, 384);
        assert_eq!(policy.dropout, 0.25);
        assert_eq!(policy.max_enc_len, 96);
    }

    #[test]
    fn checkpoint_paths_use_expected_names() {
        assert_eq!(
            checkpoint_record_path("artifacts/checkpoints", "policy_best"),
            PathBuf::from("artifacts/checkpoints").join("policy_best")
        );
        assert_eq!(
            checkpoint_metadata_path("artifacts/checkpoints", "policy_best"),
            PathBuf::from("artifacts/checkpoints").join("policy_best_metadata.json")
        );
    }

    #[test]
    fn checkpoint_metadata_has_expected_shape() {
        let metadata = checkpoint_metadata(12, 0.375);

        assert_eq!(metadata["epoch"], 12);
        assert_eq!(metadata["val_loss"], 0.375);
        assert_eq!(metadata["model_type"], "policy");
    }

    #[test]
    fn training_schedule_uses_ceiling_batches() {
        assert_eq!(
            training_schedule(65, 64, 10),
            TrainingSchedule {
                batches_per_epoch: 2,
                total_steps: 20,
            }
        );
    }

    #[test]
    fn average_epoch_loss_returns_zero_for_empty_epoch() {
        assert_eq!(average_epoch_loss(12.0, 0), 0.0);
    }

    #[test]
    fn validation_decision_resets_on_new_best() {
        assert_eq!(
            validation_decision(1.0, 3, 0.75, 5),
            ValidationDecision::NewBest
        );
    }

    #[test]
    fn validation_decision_increments_before_patience_limit() {
        assert_eq!(
            validation_decision(1.0, 1, 1.25, 4),
            ValidationDecision::Continue {
                epochs_without_improvement: 2,
            }
        );
    }

    #[test]
    fn validation_decision_stops_at_patience_limit() {
        assert_eq!(
            validation_decision(1.0, 1, 1.25, 2),
            ValidationDecision::StopEarly
        );
    }
}
