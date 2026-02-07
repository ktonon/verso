"""Training loop for the supervised simplification model."""

from __future__ import annotations

import argparse
import math
import os
import time

import torch
import torch.nn as nn
from torch.utils.data import DataLoader

from .config import TrainConfig
from .dataset import SimplificationDataset, collate_fn, load_data
from .model import SimplificationModel
from .vocab import DecoderVocab, EncoderVocab


def get_cosine_schedule_with_warmup(
    optimizer: torch.optim.Optimizer, warmup_steps: int, total_steps: int
) -> torch.optim.lr_scheduler.LambdaLR:
    def lr_lambda(step: int) -> float:
        if step < warmup_steps:
            return step / max(1, warmup_steps)
        progress = (step - warmup_steps) / max(1, total_steps - warmup_steps)
        return 0.5 * (1.0 + math.cos(math.pi * progress))

    return torch.optim.lr_scheduler.LambdaLR(optimizer, lr_lambda)


def train_epoch(
    model: SimplificationModel,
    dataloader: DataLoader,
    optimizer: torch.optim.Optimizer,
    scheduler: torch.optim.lr_scheduler.LambdaLR,
    criterion: nn.CrossEntropyLoss,
    config: TrainConfig,
    epoch: int,
) -> float:
    model.train()
    total_loss = 0.0
    device = config.device

    for i, batch in enumerate(dataloader):
        enc_ids = batch["enc_ids"].to(device)
        dec_input = batch["dec_input"].to(device)
        dec_target = batch["dec_target"].to(device)
        enc_pad_mask = batch["enc_pad_mask"].to(device)
        dec_pad_mask = batch["dec_pad_mask"].to(device)

        optimizer.zero_grad()
        logits = model(enc_ids, dec_input, enc_pad_mask, dec_pad_mask)
        loss = criterion(
            logits.reshape(-1, logits.size(-1)), dec_target.reshape(-1)
        )
        loss.backward()
        torch.nn.utils.clip_grad_norm_(model.parameters(), 1.0)
        optimizer.step()
        scheduler.step()

        total_loss += loss.item()

        if config.log_every > 0 and (i + 1) % config.log_every == 0:
            avg = total_loss / (i + 1)
            lr = scheduler.get_last_lr()[0]
            print(
                f"  epoch {epoch+1} batch {i+1}/{len(dataloader)}: "
                f"loss={avg:.4f} lr={lr:.2e}"
            )

    return total_loss / len(dataloader)


@torch.no_grad()
def validate(
    model: SimplificationModel,
    dataloader: DataLoader,
    criterion: nn.CrossEntropyLoss,
    device: str,
) -> float:
    model.eval()
    total_loss = 0.0

    for batch in dataloader:
        enc_ids = batch["enc_ids"].to(device)
        dec_input = batch["dec_input"].to(device)
        dec_target = batch["dec_target"].to(device)
        enc_pad_mask = batch["enc_pad_mask"].to(device)
        dec_pad_mask = batch["dec_pad_mask"].to(device)

        logits = model(enc_ids, dec_input, enc_pad_mask, dec_pad_mask)
        loss = criterion(
            logits.reshape(-1, logits.size(-1)), dec_target.reshape(-1)
        )
        total_loss += loss.item()

    return total_loss / len(dataloader)


def save_checkpoint(
    model: SimplificationModel,
    optimizer: torch.optim.Optimizer,
    epoch: int,
    val_loss: float,
    config: TrainConfig,
    filename: str = "best.pt",
):
    os.makedirs(config.checkpoint_dir, exist_ok=True)
    path = os.path.join(config.checkpoint_dir, filename)
    torch.save(
        {
            "epoch": epoch,
            "model_state_dict": model.state_dict(),
            "optimizer_state_dict": optimizer.state_dict(),
            "val_loss": val_loss,
            "config": config,
        },
        path,
    )
    print(f"  Checkpoint saved: {path}")


def parse_args() -> TrainConfig:
    parser = argparse.ArgumentParser(description="Train simplification model")
    config = TrainConfig()

    for field_name, field_type in config.__class__.__annotations__.items():
        default = getattr(config, field_name)
        parser.add_argument(
            f"--{field_name.replace('_', '-')}",
            type=field_type,
            default=default,
            help=f"(default: {default})",
        )

    args = parser.parse_args()
    return TrainConfig(**{k.replace("-", "_"): v for k, v in vars(args).items()})


def main():
    config = parse_args()

    torch.manual_seed(config.seed)

    # Load data
    print(f"Loading data from {config.data_dir}...")
    train_examples, val_examples = load_data(
        config.data_dir, config.val_fraction, config.seed
    )
    print(f"  Train: {len(train_examples)}, Val: {len(val_examples)}")

    enc_vocab = EncoderVocab()
    dec_vocab = DecoderVocab()
    print(f"  Encoder vocab: {enc_vocab.size()}, Decoder vocab: {dec_vocab.size()}")

    train_ds = SimplificationDataset(
        train_examples, enc_vocab, dec_vocab, config.max_actions
    )
    val_ds = SimplificationDataset(
        val_examples, enc_vocab, dec_vocab, config.max_actions
    )
    train_dl = DataLoader(
        train_ds,
        batch_size=config.batch_size,
        shuffle=True,
        collate_fn=collate_fn,
    )
    val_dl = DataLoader(
        val_ds,
        batch_size=config.batch_size,
        collate_fn=collate_fn,
    )

    # Model
    model = SimplificationModel(config, enc_vocab.size(), dec_vocab.size())
    model = model.to(config.device)
    param_count = sum(p.numel() for p in model.parameters())
    print(f"Model parameters: {param_count:,}")

    # Optimizer + scheduler
    optimizer = torch.optim.AdamW(
        model.parameters(), lr=config.lr, weight_decay=config.weight_decay
    )
    total_steps = len(train_dl) * config.max_epochs
    scheduler = get_cosine_schedule_with_warmup(
        optimizer, config.warmup_steps, total_steps
    )
    criterion = nn.CrossEntropyLoss(ignore_index=0)  # ignore PAD

    # Training loop
    best_val_loss = float("inf")
    epochs_without_improvement = 0

    print(f"\nTraining for up to {config.max_epochs} epochs on {config.device}...")
    for epoch in range(config.max_epochs):
        t0 = time.time()
        train_loss = train_epoch(
            model, train_dl, optimizer, scheduler, criterion, config, epoch
        )
        val_loss = validate(model, val_dl, criterion, config.device)
        elapsed = time.time() - t0

        print(
            f"Epoch {epoch+1}: train_loss={train_loss:.4f}, "
            f"val_loss={val_loss:.4f}, time={elapsed:.1f}s"
        )

        if val_loss < best_val_loss:
            best_val_loss = val_loss
            epochs_without_improvement = 0
            save_checkpoint(model, optimizer, epoch, val_loss, config)
        else:
            epochs_without_improvement += 1
            if epochs_without_improvement >= config.patience:
                print(
                    f"Early stopping after {epoch+1} epochs "
                    f"(no improvement for {config.patience})"
                )
                break

    print(f"\nDone. Best val_loss: {best_val_loss:.4f}")


if __name__ == "__main__":
    main()
