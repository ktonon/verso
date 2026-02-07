"""M5: Sequence Validation Harness.

Runs the trained model on validation data, validates predicted
action sequences via the Rust rule engine, and reports metrics.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass, field

import torch
from torch.utils.data import DataLoader

from .config import TrainConfig, detect_device
from .dataset import SimplificationDataset, collate_fn, load_data
from .model import SimplificationModel
from .vocab import DecoderVocab, EncoderVocab


@dataclass
class EvalConfig:
    checkpoint: str = "checkpoints/best.pt"
    data_dir: str = "data_training"
    batch_size: int = 64
    device: str = field(default_factory=detect_device)
    validate_bin: str = "target/release/validate"
    max_gen_len: int = 101
    invalid_penalty: float = 0.5
    verbose: bool = False


@dataclass
class EvalMetrics:
    total_examples: int = 0
    fully_valid_count: int = 0
    validity_rate: float = 0.0
    mean_valid_fraction: float = 0.0
    mean_complexity_delta: float = 0.0
    mean_reward: float = 0.0
    improved_count: int = 0
    unchanged_count: int = 0
    worsened_count: int = 0
    empty_sequence_count: int = 0
    mean_steps: float = 0.0


def decode_action_sequence(
    token_ids: list[int], dec_vocab: DecoderVocab
) -> list[dict]:
    """Decode model output token IDs into action dicts.

    The model produces interleaved RULE/POS tokens, then STOP.
    Returns list of {"rule_direction": int, "position": int}.
    """
    actions = []
    i = 0
    while i < len(token_ids):
        typ, val = dec_vocab.decode(token_ids[i])
        if typ == "STOP":
            break
        if typ == "RULE":
            rule_dir = val
            if i + 1 < len(token_ids):
                typ2, val2 = dec_vocab.decode(token_ids[i + 1])
                if typ2 == "POS":
                    actions.append(
                        {"rule_direction": rule_dir, "position": val2}
                    )
                    i += 2
                    continue
            break
        i += 1
    return actions


def run_validation_batch(
    entries: list[dict],
    validate_bin: str,
) -> list[dict]:
    """Send entries to the Rust validate binary and return results."""
    with tempfile.NamedTemporaryFile(
        mode="w", suffix=".jsonl", delete=False
    ) as f:
        for entry in entries:
            f.write(json.dumps(entry) + "\n")
        tmp_path = f.name

    try:
        with open(tmp_path) as stdin_file:
            result = subprocess.run(
                [validate_bin],
                stdin=stdin_file,
                capture_output=True,
                text=True,
                timeout=300,
            )
        if result.returncode != 0:
            print(f"validate binary stderr: {result.stderr}", file=sys.stderr)
            raise RuntimeError(f"validate binary failed (exit {result.returncode})")

        results = []
        for line in result.stdout.strip().split("\n"):
            if line.strip():
                results.append(json.loads(line))
        return results
    finally:
        os.unlink(tmp_path)


def compute_metrics(
    results: list[dict],
    invalid_penalty: float = 0.5,
) -> EvalMetrics:
    """Compute aggregate metrics from validation results."""
    total = len(results)
    if total == 0:
        return EvalMetrics()

    fully_valid = 0
    valid_fractions: list[float] = []
    complexity_deltas: list[float] = []
    rewards: list[float] = []
    improved = 0
    unchanged = 0
    worsened = 0
    empty = 0
    all_steps: list[int] = []

    for r in results:
        total_steps = r["total_steps"]
        valid_steps = r["valid_steps"]
        input_c = r["input_complexity"]
        final_c = r["final_complexity"]

        if total_steps == 0:
            empty += 1
            valid_fractions.append(1.0)
            complexity_deltas.append(0.0)
            rewards.append(0.0)
            all_steps.append(0)
            unchanged += 1
            continue

        if valid_steps == total_steps:
            fully_valid += 1

        valid_fractions.append(valid_steps / total_steps)

        invalid_steps = total_steps - valid_steps
        penalty = -invalid_steps * invalid_penalty if invalid_steps > 0 else 0.0
        delta = input_c - final_c
        reward = delta / max(valid_steps, 1) + penalty

        complexity_deltas.append(float(delta))
        rewards.append(reward)
        all_steps.append(total_steps)

        if final_c < input_c:
            improved += 1
        elif final_c == input_c:
            unchanged += 1
        else:
            worsened += 1

    return EvalMetrics(
        total_examples=total,
        fully_valid_count=fully_valid,
        validity_rate=fully_valid / total,
        mean_valid_fraction=sum(valid_fractions) / len(valid_fractions),
        mean_complexity_delta=sum(complexity_deltas) / len(complexity_deltas),
        mean_reward=sum(rewards) / len(rewards),
        improved_count=improved,
        unchanged_count=unchanged,
        worsened_count=worsened,
        empty_sequence_count=empty,
        mean_steps=sum(all_steps) / len(all_steps),
    )


def evaluate(
    config: EvalConfig,
    model: SimplificationModel | None = None,
) -> EvalMetrics:
    """Run full evaluation pipeline.

    If model is provided, uses it directly (for RL training eval).
    Otherwise loads from config.checkpoint.
    """
    enc_vocab = EncoderVocab(config.data_dir)
    dec_vocab = DecoderVocab(config.data_dir)

    if model is None:
        print(f"Loading checkpoint from {config.checkpoint}...")
        checkpoint = torch.load(
            config.checkpoint, map_location=config.device, weights_only=False
        )
        train_config: TrainConfig = checkpoint["config"]
        model = SimplificationModel(
            train_config, enc_vocab.size(), dec_vocab.size()
        )
        model.load_state_dict(checkpoint["model_state_dict"])
        model = model.to(config.device)
    else:
        train_config = model.config

    model.eval()

    print(f"Encoder vocab: {enc_vocab.size()}, Decoder vocab: {dec_vocab.size()}")

    # Load validation data
    _, val_examples = load_data(
        config.data_dir, train_config.val_fraction, train_config.seed
    )
    print(f"Validation examples: {len(val_examples)}")

    val_ds = SimplificationDataset(
        val_examples, enc_vocab, dec_vocab, train_config.max_actions
    )
    val_dl = DataLoader(
        val_ds, batch_size=config.batch_size, collate_fn=collate_fn
    )

    # Run inference
    print("Running model inference...")
    t0 = time.time()
    all_entries: list[dict] = []
    example_idx = 0

    with torch.no_grad():
        for batch in val_dl:
            enc_ids = batch["enc_ids"].to(config.device)
            enc_pad_mask = batch["enc_pad_mask"].to(config.device)

            predictions = model.generate(
                enc_ids,
                max_len=config.max_gen_len,
                enc_pad_mask=enc_pad_mask,
            )

            batch_size = enc_ids.size(0)
            for i in range(batch_size):
                ex = val_examples[example_idx]
                token_ids = predictions[i]
                actions = decode_action_sequence(token_ids, dec_vocab)

                all_entries.append({
                    "id": example_idx,
                    "input_tokens": ex["input_tokens"],
                    "actions": actions,
                })
                example_idx += 1

    inference_time = time.time() - t0
    print(f"Inference completed in {inference_time:.1f}s ({len(all_entries)} examples)")

    # Validate via Rust binary
    print(f"Validating {len(all_entries)} predictions via {config.validate_bin}...")
    t0 = time.time()
    raw_results = run_validation_batch(all_entries, config.validate_bin)
    validate_time = time.time() - t0
    print(f"Validation completed in {validate_time:.1f}s")

    # Match results by id
    results_by_id = {r["id"]: r for r in raw_results}
    matched_results = []
    for entry in all_entries:
        r = results_by_id.get(entry["id"])
        if r is not None:
            matched_results.append(r)
        else:
            matched_results.append({
                "valid_steps": 0,
                "total_steps": len(entry["actions"]),
                "input_complexity": 0,
                "final_complexity": 0,
            })

    metrics = compute_metrics(matched_results, config.invalid_penalty)

    # Print report
    print(f"\n{'='*40}")
    print("M5 Evaluation Report")
    print(f"{'='*40}")
    print(f"Total examples:        {metrics.total_examples}")
    print(f"Validity rate:         {metrics.validity_rate:.1%} "
          f"({metrics.fully_valid_count}/{metrics.total_examples})")
    print(f"Mean valid fraction:   {metrics.mean_valid_fraction:.1%}")
    print(f"Mean complexity delta: {metrics.mean_complexity_delta:+.2f}")
    print(f"Mean reward:           {metrics.mean_reward:+.4f}")
    print(f"Improved:              {metrics.improved_count}")
    print(f"Unchanged:             {metrics.unchanged_count}")
    print(f"Worsened:              {metrics.worsened_count}")
    print(f"Empty sequences:       {metrics.empty_sequence_count}")
    print(f"Mean steps:            {metrics.mean_steps:.1f}")

    if config.verbose:
        # Show a few example predictions
        print(f"\n{'='*40}")
        print("Sample predictions")
        print(f"{'='*40}")
        shown = 0
        for entry, result in zip(all_entries, matched_results):
            if shown >= 10:
                break
            vs = result["valid_steps"]
            ts = result["total_steps"]
            ic = result["input_complexity"]
            fc = result["final_complexity"]
            status = "VALID" if vs == ts and ts > 0 else "PARTIAL" if vs > 0 else "FAIL"
            print(f"  [{status}] {' '.join(entry['input_tokens'][:20])}{'...' if len(entry['input_tokens']) > 20 else ''}")
            print(f"    steps: {vs}/{ts}, complexity: {ic} -> {fc}")
            if result.get("output_tokens"):
                out_str = ' '.join(result["output_tokens"][:20])
                print(f"    output: {out_str}")
            shown += 1

    return metrics


def parse_args() -> EvalConfig:
    parser = argparse.ArgumentParser(description="M5 Sequence Validation")
    config = EvalConfig()
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
    return EvalConfig(**{k.replace("-", "_"): v for k, v in vars(args).items()})


def main():
    config = parse_args()
    evaluate(config)


if __name__ == "__main__":
    main()
