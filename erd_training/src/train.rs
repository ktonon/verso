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

use crate::config::TrainConfig;
use crate::dataset::{load_data, SimplificationBatch, SimplificationBatcher, SimplificationDataset};
use crate::model::SimplificationModel;
use crate::schedule::cosine_lr;
use crate::vocab::{DecoderVocab, EncoderVocab};

use erd_symbolic::random_search::IndexedRuleSet;
use erd_symbolic::RuleSet;

/// Run the full supervised training loop.
pub fn supervised_train<B: AutodiffBackend>(config: TrainConfig, device: B::Device) {
    // Load data
    println!("Loading data from {}...", config.data_dir);
    let indexed = IndexedRuleSet::new(RuleSet::full());
    let enc_vocab = EncoderVocab::new(&indexed);
    let dec_vocab = DecoderVocab::new(&indexed, 64);

    let (train_examples, val_examples) =
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

    let train_ds =
        SimplificationDataset::new(&train_examples, &enc_vocab, &dec_vocab, config.max_actions);
    let val_ds =
        SimplificationDataset::new(&val_examples, &enc_vocab, &dec_vocab, config.max_actions);

    let num_train_items = train_ds.len();

    let train_dl = DataLoaderBuilder::new(SimplificationBatcher)
        .batch_size(config.batch_size)
        .shuffle(config.seed)
        .build(train_ds);

    let val_dl = DataLoaderBuilder::new(SimplificationBatcher)
        .batch_size(config.batch_size)
        .build(val_ds);

    // Model
    let mut model: SimplificationModel<B> =
        SimplificationModel::new(&config, enc_vocab.size(), dec_vocab.size(), &device);
    println!("Model parameters: {}", model.num_params());

    // Optimizer
    let mut optimizer = AdamWConfig::new()
        .with_weight_decay(config.weight_decay as f32)
        .with_grad_clipping(Some(GradientClippingConfig::Norm(1.0)))
        .init::<B, SimplificationModel<B>>();

    // Loss
    let criterion: CrossEntropyLoss<B> = CrossEntropyLossConfig::new()
        .with_pad_tokens(Some(vec![0])) // PAD = 0
        .init(&device);

    // Schedule
    let batches_per_epoch = (num_train_items + config.batch_size - 1) / config.batch_size;
    let total_steps = batches_per_epoch * config.max_epochs;

    let mut best_val_loss = f64::INFINITY;
    let mut epochs_without_improvement = 0;
    let mut global_step = 0;

    println!(
        "\nTraining for up to {} epochs ({} batches/epoch, {} total steps)...",
        config.max_epochs, batches_per_epoch, total_steps
    );

    for epoch in 0..config.max_epochs {
        let t0 = std::time::Instant::now();
        let mut epoch_loss = 0.0f64;
        let mut num_batches = 0usize;

        for batch in train_dl.iter() {
            let lr = cosine_lr(global_step, config.warmup_steps, total_steps, config.lr);

            // Forward
            let logits = model.forward(
                batch.enc_ids,
                batch.dec_input,
                batch.enc_pad_mask,
                batch.dec_pad_mask,
            );

            // Reshape for cross entropy: [B*S, V] and [B*S]
            let [b, s, v] = logits.dims();
            let logits_flat = logits.reshape([b * s, v]);
            let targets_flat = batch.dec_target.reshape([b * s]);

            let loss = criterion.forward(logits_flat, targets_flat);
            let loss_value = loss.clone().into_data().to_vec::<f32>().unwrap()[0] as f64;
            epoch_loss += loss_value;

            // Backward + optimize
            let grads = loss.backward();
            let grads = GradientsParams::from_grads(grads, &model);
            model = optimizer.step(lr, model, grads);

            global_step += 1;
            num_batches += 1;

            if config.log_every > 0 && num_batches % config.log_every == 0 {
                println!(
                    "  epoch {} batch {}/{}: loss={:.4} lr={:.2e}",
                    epoch + 1,
                    num_batches,
                    batches_per_epoch,
                    epoch_loss / num_batches as f64,
                    lr
                );
            }
        }

        let avg_train_loss = if num_batches > 0 {
            epoch_loss / num_batches as f64
        } else {
            0.0
        };

        // Validate
        let val_loss = validate(&model, &val_dl, &criterion);

        let elapsed = t0.elapsed();
        println!(
            "Epoch {}: train_loss={:.4}, val_loss={:.4}, time={:.1}s",
            epoch + 1,
            avg_train_loss,
            val_loss,
            elapsed.as_secs_f64()
        );

        // Checkpoint
        if val_loss < best_val_loss {
            best_val_loss = val_loss;
            epochs_without_improvement = 0;
            save_checkpoint(&model, epoch, val_loss, &config.checkpoint_dir);
        } else {
            epochs_without_improvement += 1;
            if epochs_without_improvement >= config.patience {
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

/// Compute average validation loss.
fn validate<B: AutodiffBackend>(
    model: &SimplificationModel<B>,
    val_dl: &Arc<dyn DataLoader<B, SimplificationBatch<B>>>,
    criterion: &CrossEntropyLoss<B>,
) -> f64 {
    let mut total_loss = 0.0f64;
    let mut num_batches = 0usize;

    for batch in val_dl.iter() {
        let logits = model.forward(
            batch.enc_ids,
            batch.dec_input,
            batch.enc_pad_mask,
            batch.dec_pad_mask,
        );

        let [b, s, v] = logits.dims();
        let logits_flat = logits.reshape([b * s, v]);
        let targets_flat = batch.dec_target.reshape([b * s]);

        let loss = criterion.forward(logits_flat, targets_flat);
        total_loss += loss.into_data().to_vec::<f32>().unwrap()[0] as f64;
        num_batches += 1;
    }

    if num_batches > 0 {
        total_loss / num_batches as f64
    } else {
        0.0
    }
}

/// Save model checkpoint and metadata.
pub fn save_checkpoint<B: AutodiffBackend>(
    model: &SimplificationModel<B>,
    epoch: usize,
    val_loss: f64,
    checkpoint_dir: &str,
) {
    save_checkpoint_named(model, epoch, val_loss, checkpoint_dir, "best");
}

/// Save model checkpoint with a custom filename.
pub fn save_checkpoint_named<B: AutodiffBackend>(
    model: &SimplificationModel<B>,
    epoch: usize,
    val_loss: f64,
    checkpoint_dir: &str,
    name: &str,
) {
    std::fs::create_dir_all(checkpoint_dir).unwrap();

    let recorder = NamedMpkFileRecorder::<FullPrecisionSettings>::default();
    let path = PathBuf::from(checkpoint_dir).join(name);
    recorder
        .record(model.clone().into_record(), path)
        .expect("Failed to save model checkpoint");

    // Save metadata
    let metadata = serde_json::json!({
        "epoch": epoch,
        "val_loss": val_loss,
    });
    let meta_path = PathBuf::from(checkpoint_dir).join(format!("{}_metadata.json", name));
    std::fs::write(meta_path, serde_json::to_string_pretty(&metadata).unwrap()).unwrap();

    println!("  Checkpoint saved: {}/{}.mpk", checkpoint_dir, name);
}

/// Load a model from a checkpoint file.
///
/// The `checkpoint_path` should be the path without the `.mpk` extension
/// (e.g., "checkpoints/best").
pub fn load_model<B: Backend>(
    config: &TrainConfig,
    enc_vocab_size: usize,
    dec_vocab_size: usize,
    device: &B::Device,
    checkpoint_path: &str,
) -> SimplificationModel<B> {
    let recorder = NamedMpkFileRecorder::<FullPrecisionSettings>::default();
    let record = recorder
        .load(PathBuf::from(checkpoint_path), device)
        .expect("Failed to load model checkpoint");
    let model = SimplificationModel::new(config, enc_vocab_size, dec_vocab_size, device);
    model.load_record(record)
}
