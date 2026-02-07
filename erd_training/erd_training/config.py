from dataclasses import dataclass


@dataclass
class TrainConfig:
    # Data
    data_dir: str = "data_training"
    val_fraction: float = 0.1
    max_actions: int = 50
    seed: int = 42

    # Model
    d_model: int = 128
    n_encoder_layers: int = 4
    n_decoder_layers: int = 4
    n_heads: int = 4
    d_ff: int = 256
    dropout: float = 0.1
    max_enc_len: int = 64
    max_dec_len: int = 128

    # Training
    batch_size: int = 64
    lr: float = 3e-4
    weight_decay: float = 0.01
    warmup_steps: int = 200
    max_epochs: int = 100
    patience: int = 10
    checkpoint_dir: str = "checkpoints"
    log_every: int = 50

    # Device
    device: str = "cpu"
