use clap::Parser;

/// Configuration for supervised training.
#[derive(Parser, Debug, Clone)]
pub struct TrainConfig {
    // Data
    #[arg(long, default_value = "data_training")]
    pub data_dir: String,
    #[arg(long, default_value_t = 0.1)]
    pub val_fraction: f64,
    #[arg(long, default_value_t = 50)]
    pub max_actions: usize,
    #[arg(long, default_value_t = 42)]
    pub seed: u64,

    // Model
    #[arg(long, default_value_t = 128)]
    pub d_model: usize,
    #[arg(long, default_value_t = 4)]
    pub n_encoder_layers: usize,
    #[arg(long, default_value_t = 4)]
    pub n_decoder_layers: usize,
    #[arg(long, default_value_t = 4)]
    pub n_heads: usize,
    #[arg(long, default_value_t = 256)]
    pub d_ff: usize,
    #[arg(long, default_value_t = 0.1)]
    pub dropout: f64,
    #[arg(long, default_value_t = 64)]
    pub max_enc_len: usize,
    #[arg(long, default_value_t = 128)]
    pub max_dec_len: usize,

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

    // Device
    #[arg(long, default_value = "cpu")]
    pub device: String,
}

/// Configuration for evaluation.
#[derive(Parser, Debug, Clone)]
pub struct EvalConfig {
    #[arg(long, default_value = "checkpoints/best")]
    pub checkpoint: String,
    #[arg(long, default_value = "data_training")]
    pub data_dir: String,
    #[arg(long, default_value_t = 64)]
    pub batch_size: usize,
    #[arg(long, default_value_t = 0.1)]
    pub val_fraction: f64,
    #[arg(long, default_value_t = 42)]
    pub seed: u64,
    #[arg(long, default_value = "cpu")]
    pub device: String,
    #[arg(long, default_value_t = 0.5)]
    pub invalid_penalty: f64,

    // Model architecture (must match training config)
    #[arg(long, default_value_t = 128)]
    pub d_model: usize,
    #[arg(long, default_value_t = 4)]
    pub n_encoder_layers: usize,
    #[arg(long, default_value_t = 4)]
    pub n_decoder_layers: usize,
    #[arg(long, default_value_t = 4)]
    pub n_heads: usize,
    #[arg(long, default_value_t = 256)]
    pub d_ff: usize,
    #[arg(long, default_value_t = 0.1)]
    pub dropout: f64,
    #[arg(long, default_value_t = 64)]
    pub max_enc_len: usize,
    #[arg(long, default_value_t = 128)]
    pub max_dec_len: usize,
    #[arg(long, default_value_t = 64)]
    pub max_positions: usize,
}

impl EvalConfig {
    /// Convert to a TrainConfig for model loading.
    pub fn to_train_config(&self) -> TrainConfig {
        TrainConfig {
            data_dir: self.data_dir.clone(),
            val_fraction: self.val_fraction,
            max_actions: 50,
            seed: self.seed,
            d_model: self.d_model,
            n_encoder_layers: self.n_encoder_layers,
            n_decoder_layers: self.n_decoder_layers,
            n_heads: self.n_heads,
            d_ff: self.d_ff,
            dropout: self.dropout,
            max_enc_len: self.max_enc_len,
            max_dec_len: self.max_dec_len,
            batch_size: self.batch_size,
            lr: 3e-4,
            weight_decay: 0.01,
            warmup_steps: 200,
            max_epochs: 100,
            patience: 10,
            checkpoint_dir: String::new(),
            log_every: 50,
            device: self.device.clone(),
        }
    }
}

/// Configuration for REINFORCE training.
#[derive(Parser, Debug, Clone)]
pub struct RLConfig {
    // Checkpoint to initialize from
    #[arg(long, default_value = "checkpoints/best")]
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
    #[arg(long, default_value_t = 101)]
    pub max_gen_len: usize,

    // Infrastructure
    #[arg(long, default_value = "checkpoints")]
    pub checkpoint_dir: String,
    #[arg(long, default_value = "cpu")]
    pub device: String,
    #[arg(long, default_value_t = 10)]
    pub log_every: usize,

    // Data (for evaluation splits)
    #[arg(long, default_value_t = 0.1)]
    pub val_fraction: f64,
    #[arg(long, default_value_t = 42)]
    pub seed: u64,
    #[arg(long, default_value_t = 64)]
    pub max_positions: usize,

    // Model architecture (must match training config)
    #[arg(long, default_value_t = 128)]
    pub d_model: usize,
    #[arg(long, default_value_t = 4)]
    pub n_encoder_layers: usize,
    #[arg(long, default_value_t = 4)]
    pub n_decoder_layers: usize,
    #[arg(long, default_value_t = 4)]
    pub n_heads: usize,
    #[arg(long, default_value_t = 256)]
    pub d_ff: usize,
    #[arg(long, default_value_t = 0.1)]
    pub dropout: f64,
    #[arg(long, default_value_t = 64)]
    pub max_enc_len: usize,
    #[arg(long, default_value_t = 128)]
    pub max_dec_len: usize,
}

impl RLConfig {
    /// Convert to a TrainConfig for model loading.
    pub fn to_train_config(&self) -> TrainConfig {
        TrainConfig {
            data_dir: self.data_dir.clone(),
            val_fraction: self.val_fraction,
            max_actions: 50,
            seed: self.seed,
            d_model: self.d_model,
            n_encoder_layers: self.n_encoder_layers,
            n_decoder_layers: self.n_decoder_layers,
            n_heads: self.n_heads,
            d_ff: self.d_ff,
            dropout: self.dropout,
            max_enc_len: self.max_enc_len,
            max_dec_len: self.max_dec_len,
            batch_size: self.batch_size,
            lr: self.lr,
            weight_decay: 0.01,
            warmup_steps: 0,
            max_epochs: self.max_epochs,
            patience: self.max_epochs,
            checkpoint_dir: self.checkpoint_dir.clone(),
            log_every: self.log_every,
            device: self.device.clone(),
        }
    }
}
