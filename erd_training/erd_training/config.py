from dataclasses import dataclass, field

import torch


def detect_device() -> str:
    """Auto-detect best available device: cuda > mps > cpu."""
    if torch.cuda.is_available():
        return "cuda"
    if hasattr(torch.backends, "mps") and torch.backends.mps.is_available():
        return "mps"
    return "cpu"


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
    device: str = field(default_factory=detect_device)
