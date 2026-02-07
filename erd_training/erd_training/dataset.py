"""JSONL data loading and PyTorch Dataset for simplification traces."""

from __future__ import annotations

import glob
import json
import random

import torch
from torch.nn.utils.rnn import pad_sequence
from torch.utils.data import Dataset

from .vocab import DecoderVocab, EncoderVocab


def load_data(
    data_dir: str, val_fraction: float, seed: int
) -> tuple[list[dict], list[dict]]:
    """Load all JSONL files from data_dir, shuffle, split into train/val."""
    files = sorted(glob.glob(f"{data_dir}/*.jsonl"))
    if not files:
        raise FileNotFoundError(f"No .jsonl files found in {data_dir}")

    examples = []
    for path in files:
        with open(path) as f:
            for line in f:
                examples.append(json.loads(line))

    rng = random.Random(seed)
    rng.shuffle(examples)

    split = int(len(examples) * (1 - val_fraction))
    return examples[:split], examples[split:]


class SimplificationDataset(Dataset):
    """Converts JSONL examples to tensor format for training."""

    def __init__(
        self,
        examples: list[dict],
        enc_vocab: EncoderVocab,
        dec_vocab: DecoderVocab,
        max_actions: int = 50,
    ):
        self.examples = examples
        self.enc_vocab = enc_vocab
        self.dec_vocab = dec_vocab
        self.max_actions = max_actions

    def __len__(self):
        return len(self.examples)

    def __getitem__(self, idx: int) -> dict:
        ex = self.examples[idx]

        # Encode input tokens
        enc_ids = [self.enc_vocab.encode(t) for t in ex["input_tokens"]]

        # Build decoder sequence: interleaved RULE/POS tokens + STOP
        actions = ex["actions"][: self.max_actions]
        dec_tokens = []
        for action in actions:
            dec_tokens.append(self.dec_vocab.encode_rule(action["rule_direction"]))
            dec_tokens.append(self.dec_vocab.encode_pos(action["position"]))
        dec_tokens.append(self.dec_vocab.STOP)

        # Teacher forcing: input is BOS + all but last, target is the full sequence
        dec_input = [self.dec_vocab.BOS] + dec_tokens[:-1]
        dec_target = dec_tokens

        return {
            "enc_ids": torch.tensor(enc_ids, dtype=torch.long),
            "dec_input": torch.tensor(dec_input, dtype=torch.long),
            "dec_target": torch.tensor(dec_target, dtype=torch.long),
        }


def collate_fn(batch: list[dict]) -> dict:
    """Pad variable-length sequences and create padding masks."""
    enc_ids = pad_sequence(
        [b["enc_ids"] for b in batch], batch_first=True, padding_value=0
    )
    dec_input = pad_sequence(
        [b["dec_input"] for b in batch], batch_first=True, padding_value=0
    )
    dec_target = pad_sequence(
        [b["dec_target"] for b in batch], batch_first=True, padding_value=0
    )

    enc_pad_mask = enc_ids == 0
    dec_pad_mask = dec_input == 0

    return {
        "enc_ids": enc_ids,
        "dec_input": dec_input,
        "dec_target": dec_target,
        "enc_pad_mask": enc_pad_mask,
        "dec_pad_mask": dec_pad_mask,
    }
