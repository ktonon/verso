"""M6: REINFORCE training for expression simplification.

Trains the model via policy gradient on self-generated action sequences,
validated by the Rust rule engine. Initializes from a Phase 1 (supervised)
checkpoint.
"""

from __future__ import annotations

import argparse
import random
import time
from dataclasses import dataclass, field

import torch
import torch.nn.functional as F
from torch.nn.utils.rnn import pad_sequence

from .config import TrainConfig, detect_device
from .evaluate import (
    EvalConfig,
    decode_action_sequence,
    evaluate,
    run_validation_batch,
)
from .model import SimplificationModel
from .train import save_checkpoint
from .vocab import DecoderVocab, EncoderVocab


@dataclass
class RLConfig:
    # Initialization
    checkpoint: str = "checkpoints/best.pt"
    data_dir: str = "data_training"

    # RL hyperparameters
    batch_size: int = 32
    lr: float = 1e-5
    temperature: float = 1.0
    max_epochs: int = 50
    eval_every: int = 5
    baseline_decay: float = 0.99
    entropy_bonus: float = 0.05
    invalid_penalty: float = 0.5
    max_gen_len: int = 101

    # Infrastructure
    validate_bin: str = "target/release/validate"
    checkpoint_dir: str = "checkpoints"
    device: str = field(default_factory=detect_device)
    log_every: int = 10


def compute_reward(
    result: dict,
    invalid_penalty: float,
) -> float:
    """Compute reward for a single validated trajectory.

    Same formula as evaluate.py compute_metrics, but per-example.
    """
    total_steps = result["total_steps"]
    valid_steps = result["valid_steps"]
    input_c = result["input_complexity"]
    final_c = result["final_complexity"]

    if total_steps == 0:
        return 0.0

    invalid_steps = total_steps - valid_steps
    penalty = -invalid_steps * invalid_penalty if invalid_steps > 0 else 0.0
    delta = input_c - final_c
    return delta / max(valid_steps, 1) + penalty


def encode_expressions(
    examples: list[dict],
    enc_vocab: EncoderVocab,
    device: str,
) -> tuple[torch.Tensor, torch.Tensor]:
    """Encode a batch of expressions into padded tensors."""
    enc_ids_list = []
    for ex in examples:
        ids = [enc_vocab.encode(t) for t in ex["input_tokens"]]
        enc_ids_list.append(torch.tensor(ids, dtype=torch.long))

    enc_ids = pad_sequence(enc_ids_list, batch_first=True, padding_value=0)
    enc_pad_mask = enc_ids == 0
    return enc_ids.to(device), enc_pad_mask.to(device)


def rl_train_step(
    model: SimplificationModel,
    batch_examples: list[dict],
    enc_vocab: EncoderVocab,
    dec_vocab: DecoderVocab,
    optimizer: torch.optim.Optimizer,
    config: RLConfig,
    baseline: float,
) -> tuple[float, float, float, int]:
    """One REINFORCE training step.

    Returns (loss_value, mean_reward, new_baseline, num_valid).
    """
    device = config.device

    # 1. Encode input expressions
    enc_ids, enc_pad_mask = encode_expressions(batch_examples, enc_vocab, device)

    # 2. Sample action sequences (no grad)
    sampled_ids = model.sample(
        enc_ids,
        max_len=config.max_gen_len,
        enc_pad_mask=enc_pad_mask,
        temperature=config.temperature,
    )

    # 3. Decode sampled tokens into action dicts
    all_actions = [decode_action_sequence(ids, dec_vocab) for ids in sampled_ids]

    # 4. Validate via Rust binary
    entries = []
    for i, ex in enumerate(batch_examples):
        entries.append({
            "id": i,
            "input_tokens": ex["input_tokens"],
            "actions": all_actions[i],
        })

    raw_results = run_validation_batch(entries, config.validate_bin)
    results_by_id = {r["id"]: r for r in raw_results}

    # 5. Compute per-example rewards
    rewards = []
    num_valid = 0
    for i, entry in enumerate(entries):
        r = results_by_id.get(entry["id"])
        if r is not None:
            reward = compute_reward(r, config.invalid_penalty)
            if r["valid_steps"] == r["total_steps"] and r["total_steps"] > 0:
                num_valid += 1
        else:
            reward = -len(entry["actions"]) * config.invalid_penalty
        rewards.append(reward)

    reward_tensor = torch.tensor(rewards, dtype=torch.float32, device=device)
    mean_reward = reward_tensor.mean().item()

    # 6. Forward pass with sampled sequences to get log probabilities
    # Build dec_input (BOS + sampled[:-1]) and dec_target (sampled)
    dec_input_list = []
    dec_target_list = []
    seq_lengths = []
    for ids in sampled_ids:
        target = ids[:]  # include STOP if present
        inp = [dec_vocab.BOS] + target[:-1]
        dec_input_list.append(torch.tensor(inp, dtype=torch.long))
        dec_target_list.append(torch.tensor(target, dtype=torch.long))
        seq_lengths.append(len(target))

    dec_input = pad_sequence(
        dec_input_list, batch_first=True, padding_value=0
    ).to(device)
    dec_target = pad_sequence(
        dec_target_list, batch_first=True, padding_value=0
    ).to(device)
    dec_pad_mask = dec_input == 0

    model.train()
    logits = model(enc_ids, dec_input, enc_pad_mask, dec_pad_mask)
    # logits: [B, S_dec, V]

    # Per-token log probabilities
    log_probs = F.log_softmax(logits, dim=-1)
    # Gather log probs for the sampled tokens
    token_log_probs = log_probs.gather(
        2, dec_target.unsqueeze(-1)
    ).squeeze(-1)  # [B, S_dec]

    # Mask padding positions
    mask = dec_target != 0  # [B, S_dec], True for real tokens
    token_log_probs = token_log_probs * mask.float()

    # Sum log probs per trajectory
    trajectory_log_probs = token_log_probs.sum(dim=1)  # [B]

    # 7. Compute advantages with per-batch normalization
    advantages = reward_tensor - baseline
    if len(advantages) > 1 and advantages.std() > 1e-8:
        advantages = advantages / (advantages.std() + 1e-8)

    # 8. REINFORCE loss: -log_prob * advantage
    # Normalize trajectory log probs by sequence length to prevent
    # loss magnitude from scaling with sequence length
    seq_len_tensor = torch.tensor(
        seq_lengths, dtype=torch.float32, device=device
    ).clamp(min=1)
    normalized_log_probs = trajectory_log_probs / seq_len_tensor
    rl_loss = -(normalized_log_probs * advantages).mean()

    # 9. Entropy bonus (encourage exploration)
    probs = F.softmax(logits, dim=-1)
    entropy = -(probs * log_probs).sum(dim=-1)  # [B, S_dec]
    mean_entropy = (entropy * mask.float()).sum() / mask.float().sum()
    loss = rl_loss - config.entropy_bonus * mean_entropy

    # 10. Update
    optimizer.zero_grad()
    loss.backward()
    torch.nn.utils.clip_grad_norm_(model.parameters(), 1.0)
    optimizer.step()

    # 11. Update baseline
    new_baseline = config.baseline_decay * baseline + (
        1 - config.baseline_decay
    ) * mean_reward

    return loss.item(), mean_reward, new_baseline, num_valid


def rl_train(config: RLConfig) -> None:
    """Full REINFORCE training loop."""
    print(f"Loading Phase 1 checkpoint from {config.checkpoint}...")
    checkpoint = torch.load(
        config.checkpoint, map_location=config.device, weights_only=False
    )
    train_config: TrainConfig = checkpoint["config"]

    enc_vocab = EncoderVocab(config.data_dir)
    dec_vocab = DecoderVocab(config.data_dir)

    model = SimplificationModel(
        train_config, enc_vocab.size(), dec_vocab.size()
    )
    model.load_state_dict(checkpoint["model_state_dict"])
    model = model.to(config.device)

    print(
        f"Encoder vocab: {enc_vocab.size()}, "
        f"Decoder vocab: {dec_vocab.size()}"
    )

    # Load training expressions (we only need input_tokens)
    from .dataset import load_data

    all_examples, _ = load_data(
        config.data_dir, val_fraction=0.1, seed=train_config.seed
    )
    print(f"Training expressions: {len(all_examples)}")

    optimizer = torch.optim.AdamW(
        model.parameters(), lr=config.lr, weight_decay=0.01
    )

    baseline = 0.0
    best_eval_reward = float("-inf")
    rng = random.Random(config.data_dir)

    print(
        f"\nRL training for up to {config.max_epochs} epochs "
        f"on {config.device}..."
    )
    print(f"  batch_size={config.batch_size}, lr={config.lr}, "
          f"temperature={config.temperature}")
    print(f"  entropy_bonus={config.entropy_bonus}, "
          f"invalid_penalty={config.invalid_penalty}")

    for epoch in range(config.max_epochs):
        t0 = time.time()
        rng.shuffle(all_examples)

        epoch_loss = 0.0
        epoch_reward = 0.0
        epoch_valid = 0
        num_batches = 0

        for start in range(0, len(all_examples), config.batch_size):
            batch = all_examples[start : start + config.batch_size]
            if len(batch) < 2:
                continue

            loss, mean_reward, baseline, num_valid = rl_train_step(
                model, batch, enc_vocab, dec_vocab, optimizer, config, baseline
            )

            epoch_loss += loss
            epoch_reward += mean_reward
            epoch_valid += num_valid
            num_batches += 1

            if config.log_every > 0 and num_batches % config.log_every == 0:
                avg_loss = epoch_loss / num_batches
                avg_reward = epoch_reward / num_batches
                print(
                    f"  epoch {epoch+1} batch {num_batches}: "
                    f"loss={avg_loss:.4f} reward={avg_reward:+.3f} "
                    f"baseline={baseline:.3f}"
                )

        elapsed = time.time() - t0
        avg_loss = epoch_loss / max(num_batches, 1)
        avg_reward = epoch_reward / max(num_batches, 1)
        total_examples = num_batches * config.batch_size
        validity_rate = epoch_valid / max(total_examples, 1)

        print(
            f"Epoch {epoch+1}: loss={avg_loss:.4f}, "
            f"reward={avg_reward:+.3f}, validity={validity_rate:.1%}, "
            f"baseline={baseline:.3f}, time={elapsed:.1f}s"
        )

        # Periodic full evaluation
        if (epoch + 1) % config.eval_every == 0:
            print(f"\n--- Full evaluation at epoch {epoch+1} ---")
            eval_config = EvalConfig(
                checkpoint="",  # unused, we pass model directly
                data_dir=config.data_dir,
                batch_size=config.batch_size * 2,
                device=config.device,
                validate_bin=config.validate_bin,
                max_gen_len=config.max_gen_len,
                invalid_penalty=config.invalid_penalty,
            )
            metrics = evaluate(eval_config, model=model)

            if metrics.mean_reward > best_eval_reward:
                best_eval_reward = metrics.mean_reward
                save_checkpoint(
                    model,
                    optimizer,
                    epoch,
                    -metrics.mean_reward,  # val_loss field, lower is better
                    train_config,
                    filename="rl_best.pt",
                )
            print()

    # Always save final checkpoint
    save_checkpoint(
        model,
        optimizer,
        config.max_epochs - 1,
        -baseline,
        train_config,
        filename="rl_latest.pt",
    )

    print(f"\nDone. Best eval reward: {best_eval_reward:+.4f}")


def parse_args() -> RLConfig:
    parser = argparse.ArgumentParser(description="M6 REINFORCE Training")
    config = RLConfig()
    for field_name in config.__class__.__annotations__:
        default = getattr(config, field_name)
        if isinstance(default, bool):
            parser.add_argument(
                f"--{field_name.replace('_', '-')}",
                action="store_true",
                default=default,
            )
        else:
            parser.add_argument(
                f"--{field_name.replace('_', '-')}",
                type=type(default),
                default=default,
            )
    args = parser.parse_args()
    return RLConfig(**{k.replace("-", "_"): v for k, v in vars(args).items()})


def main():
    config = parse_args()
    rl_train(config)


if __name__ == "__main__":
    main()
