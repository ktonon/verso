"""Transformer encoder-decoder for expression simplification."""

from __future__ import annotations

import math

import torch
import torch.nn as nn
import torch.nn.functional as F

from .config import TrainConfig


class SimplificationModel(nn.Module):
    def __init__(
        self, config: TrainConfig, enc_vocab_size: int, dec_vocab_size: int
    ):
        super().__init__()
        self.config = config
        self.dec_vocab_size = dec_vocab_size

        # Encoder
        self.enc_tok_emb = nn.Embedding(
            enc_vocab_size, config.d_model, padding_idx=0
        )
        self.enc_pos_emb = nn.Embedding(config.max_enc_len, config.d_model)
        encoder_layer = nn.TransformerEncoderLayer(
            d_model=config.d_model,
            nhead=config.n_heads,
            dim_feedforward=config.d_ff,
            dropout=config.dropout,
            batch_first=True,
        )
        self.encoder = nn.TransformerEncoder(
            encoder_layer,
            num_layers=config.n_encoder_layers,
            enable_nested_tensor=False,  # required for MPS device support
        )

        # Decoder
        self.dec_tok_emb = nn.Embedding(
            dec_vocab_size, config.d_model, padding_idx=0
        )
        self.dec_pos_emb = nn.Embedding(config.max_dec_len, config.d_model)
        decoder_layer = nn.TransformerDecoderLayer(
            d_model=config.d_model,
            nhead=config.n_heads,
            dim_feedforward=config.d_ff,
            dropout=config.dropout,
            batch_first=True,
        )
        self.decoder = nn.TransformerDecoder(
            decoder_layer, num_layers=config.n_decoder_layers
        )

        # Output projection
        self.output_proj = nn.Linear(config.d_model, dec_vocab_size)

        self._init_weights()

    def _init_weights(self):
        for p in self.parameters():
            if p.dim() > 1:
                nn.init.xavier_uniform_(p)

    def forward(
        self,
        enc_ids: torch.Tensor,
        dec_input: torch.Tensor,
        enc_pad_mask: torch.Tensor | None = None,
        dec_pad_mask: torch.Tensor | None = None,
    ) -> torch.Tensor:
        """
        Args:
            enc_ids: [B, S_enc] encoder token IDs
            dec_input: [B, S_dec] decoder input IDs (BOS + shifted target)
            enc_pad_mask: [B, S_enc] True where padded
            dec_pad_mask: [B, S_dec] True where padded

        Returns:
            logits: [B, S_dec, dec_vocab_size]
        """
        device = enc_ids.device

        # Encoder
        enc_len = enc_ids.size(1)
        enc_pos = torch.arange(enc_len, device=device).unsqueeze(0)
        enc_emb = self.enc_tok_emb(enc_ids) + self.enc_pos_emb(enc_pos)
        memory = self.encoder(enc_emb, src_key_padding_mask=enc_pad_mask)

        # Decoder
        dec_len = dec_input.size(1)
        dec_pos = torch.arange(dec_len, device=device).unsqueeze(0)
        dec_emb = self.dec_tok_emb(dec_input) + self.dec_pos_emb(dec_pos)
        causal_mask = nn.Transformer.generate_square_subsequent_mask(
            dec_len, device=device
        )
        output = self.decoder(
            dec_emb,
            memory,
            tgt_mask=causal_mask,
            tgt_key_padding_mask=dec_pad_mask,
            memory_key_padding_mask=enc_pad_mask,
        )

        return self.output_proj(output)

    @torch.no_grad()
    def generate(
        self,
        enc_ids: torch.Tensor,
        max_len: int = 101,
        enc_pad_mask: torch.Tensor | None = None,
        stop_token: int = 2,
    ) -> list[list[int]]:
        """Greedy autoregressive generation.

        Args:
            enc_ids: [B, S_enc]
            max_len: max decoder tokens to generate
            enc_pad_mask: [B, S_enc]
            stop_token: decoder STOP token ID

        Returns:
            List of token ID sequences (one per batch element), excluding BOS.
        """
        self.eval()
        device = enc_ids.device
        batch_size = enc_ids.size(0)

        # Encode once
        enc_len = enc_ids.size(1)
        enc_pos = torch.arange(enc_len, device=device).unsqueeze(0)
        enc_emb = self.enc_tok_emb(enc_ids) + self.enc_pos_emb(enc_pos)
        memory = self.encoder(enc_emb, src_key_padding_mask=enc_pad_mask)

        # Start with BOS
        generated = torch.full(
            (batch_size, 1), 1, dtype=torch.long, device=device
        )  # BOS=1
        finished = torch.zeros(batch_size, dtype=torch.bool, device=device)

        for _ in range(max_len):
            dec_len = generated.size(1)
            dec_pos = torch.arange(dec_len, device=device).unsqueeze(0)
            dec_emb = self.dec_tok_emb(generated) + self.dec_pos_emb(dec_pos)
            causal_mask = nn.Transformer.generate_square_subsequent_mask(
                dec_len, device=device
            )
            output = self.decoder(
                dec_emb,
                memory,
                tgt_mask=causal_mask,
                memory_key_padding_mask=enc_pad_mask,
            )
            logits = self.output_proj(output[:, -1, :])  # [B, V]
            next_token = logits.argmax(dim=-1, keepdim=True)  # [B, 1]

            # Mark as finished if STOP predicted
            finished |= next_token.squeeze(-1) == stop_token

            generated = torch.cat([generated, next_token], dim=1)

            if finished.all():
                break

        # Convert to lists, strip BOS, truncate at STOP
        results = []
        for i in range(batch_size):
            tokens = generated[i, 1:].tolist()  # skip BOS
            try:
                stop_idx = tokens.index(stop_token)
                tokens = tokens[: stop_idx + 1]
            except ValueError:
                pass
            results.append(tokens)

        return results

    @torch.no_grad()
    def sample(
        self,
        enc_ids: torch.Tensor,
        max_len: int = 101,
        enc_pad_mask: torch.Tensor | None = None,
        stop_token: int = 2,
        temperature: float = 1.0,
    ) -> list[list[int]]:
        """Sample action sequences for REINFORCE rollouts.

        Like generate() but samples from the softmax distribution
        instead of taking argmax. Used for on-policy exploration.

        Args:
            enc_ids: [B, S_enc]
            max_len: max decoder tokens to generate
            enc_pad_mask: [B, S_enc]
            stop_token: decoder STOP token ID
            temperature: softmax temperature (higher = more random)

        Returns:
            List of token ID sequences (one per batch element), excluding BOS.
        """
        self.eval()
        device = enc_ids.device
        batch_size = enc_ids.size(0)

        # Encode once
        enc_len = enc_ids.size(1)
        enc_pos = torch.arange(enc_len, device=device).unsqueeze(0)
        enc_emb = self.enc_tok_emb(enc_ids) + self.enc_pos_emb(enc_pos)
        memory = self.encoder(enc_emb, src_key_padding_mask=enc_pad_mask)

        # Start with BOS
        generated = torch.full(
            (batch_size, 1), 1, dtype=torch.long, device=device
        )  # BOS=1
        finished = torch.zeros(batch_size, dtype=torch.bool, device=device)

        for _ in range(max_len):
            dec_len = generated.size(1)
            dec_pos = torch.arange(dec_len, device=device).unsqueeze(0)
            dec_emb = self.dec_tok_emb(generated) + self.dec_pos_emb(dec_pos)
            causal_mask = nn.Transformer.generate_square_subsequent_mask(
                dec_len, device=device
            )
            output = self.decoder(
                dec_emb,
                memory,
                tgt_mask=causal_mask,
                memory_key_padding_mask=enc_pad_mask,
            )
            logits = self.output_proj(output[:, -1, :])  # [B, V]
            logits = logits.clamp(-50, 50)  # prevent inf/nan after RL updates
            probs = F.softmax(logits / temperature, dim=-1)
            next_token = torch.multinomial(probs, num_samples=1)  # [B, 1]

            # Mark as finished if STOP predicted
            finished |= next_token.squeeze(-1) == stop_token

            generated = torch.cat([generated, next_token], dim=1)

            if finished.all():
                break

        # Convert to lists, strip BOS, truncate at STOP
        results = []
        for i in range(batch_size):
            tokens = generated[i, 1:].tolist()  # skip BOS
            try:
                stop_idx = tokens.index(stop_token)
                tokens = tokens[: stop_idx + 1]
            except ValueError:
                pass
            results.append(tokens)

        return results
