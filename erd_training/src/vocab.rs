use erd_symbolic::random_search::IndexedRuleSet;
use erd_symbolic::training_data::build_vocab_metadata;

/// Encoder vocabulary: maps input token strings to integer IDs.
///
/// Layout: [PAD=0, token_0=1, token_1=2, ...]
/// Built directly from `erd_symbolic` types — no vocab.json needed.
pub struct EncoderVocab {
    token_to_id: std::collections::HashMap<String, usize>,
    id_to_token: Vec<String>,
}

impl EncoderVocab {
    pub const PAD: usize = 0;

    /// Build encoder vocabulary from the IndexedRuleSet.
    pub fn new(indexed: &IndexedRuleSet) -> Self {
        let metadata = build_vocab_metadata(indexed);
        let mut token_to_id = std::collections::HashMap::new();
        let mut id_to_token = vec!["<PAD>".to_string()];

        token_to_id.insert("<PAD>".to_string(), 0);
        for tok in &metadata.encoder_tokens {
            let idx = id_to_token.len();
            token_to_id.insert(tok.clone(), idx);
            id_to_token.push(tok.clone());
        }

        Self {
            token_to_id,
            id_to_token,
        }
    }

    /// Convert a token string to its integer ID. Returns PAD for unknown tokens.
    pub fn encode(&self, token: &str) -> usize {
        self.token_to_id.get(token).copied().unwrap_or(Self::PAD)
    }

    /// Convert an integer ID to its token string.
    pub fn decode(&self, token_id: usize) -> &str {
        self.id_to_token
            .get(token_id)
            .map(|s| s.as_str())
            .unwrap_or("<PAD>")
    }

    /// Total vocabulary size (including PAD).
    pub fn size(&self) -> usize {
        self.id_to_token.len()
    }
}

/// Decoder vocabulary: maps action tokens (BOS, STOP, RULE_n, POS_n) to integer IDs.
///
/// Layout: [PAD=0, BOS=1, STOP=2, RULE_0..RULE_{n-1}, POS_0..POS_{m-1}]
/// Built directly from `IndexedRuleSet::total_directions` — no vocab.json needed.
pub struct DecoderVocab {
    num_rules: usize,
    max_positions: usize,
}

impl DecoderVocab {
    pub const PAD: usize = 0;
    pub const BOS: usize = 1;
    pub const STOP: usize = 2;
    const RULE_OFFSET: usize = 3; // after PAD, BOS, STOP

    /// Build decoder vocabulary from the IndexedRuleSet.
    pub fn new(indexed: &IndexedRuleSet, max_positions: usize) -> Self {
        Self {
            num_rules: indexed.total_directions as usize,
            max_positions,
        }
    }

    /// Offset where position tokens start.
    pub fn pos_offset(&self) -> usize {
        Self::RULE_OFFSET + self.num_rules
    }

    /// Encode a rule direction ID to a decoder token ID.
    pub fn encode_rule(&self, rule_dir: u16) -> usize {
        Self::RULE_OFFSET + rule_dir as usize
    }

    /// Encode a position index to a decoder token ID.
    pub fn encode_pos(&self, pos: usize) -> usize {
        self.pos_offset() + pos.min(self.max_positions - 1)
    }

    /// Decode a token ID into its type and value.
    pub fn decode(&self, token_id: usize) -> DecoderToken {
        if token_id == Self::PAD {
            DecoderToken::Pad
        } else if token_id == Self::BOS {
            DecoderToken::Bos
        } else if token_id == Self::STOP {
            DecoderToken::Stop
        } else if token_id >= Self::RULE_OFFSET && token_id < self.pos_offset() {
            DecoderToken::Rule(token_id - Self::RULE_OFFSET)
        } else if token_id >= self.pos_offset() && token_id < self.pos_offset() + self.max_positions
        {
            DecoderToken::Pos(token_id - self.pos_offset())
        } else {
            DecoderToken::Pad
        }
    }

    /// Total vocabulary size.
    pub fn size(&self) -> usize {
        Self::RULE_OFFSET + self.num_rules + self.max_positions
    }
}

/// Decoded decoder token type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecoderToken {
    Pad,
    Bos,
    Stop,
    Rule(usize),
    Pos(usize),
}

#[cfg(test)]
mod tests {
    use super::*;
    use erd_symbolic::RuleSet;

    fn make_vocabs() -> (EncoderVocab, DecoderVocab) {
        let indexed = IndexedRuleSet::new(RuleSet::full());
        let enc = EncoderVocab::new(&indexed);
        let dec = DecoderVocab::new(&indexed, 64);
        (enc, dec)
    }

    #[test]
    fn encoder_vocab_size_matches_python() {
        let (enc, _) = make_vocabs();
        // Python's EncoderVocab: 75 tokens + PAD = 76
        assert_eq!(enc.size(), 76);
    }

    #[test]
    fn decoder_vocab_size_matches_python() {
        let (_, dec) = make_vocabs();
        // Python's DecoderVocab: PAD + BOS + STOP + total_directions + 64 positions
        let indexed = IndexedRuleSet::new(RuleSet::full());
        let expected = 3 + indexed.total_directions as usize + 64;
        assert_eq!(dec.size(), expected);
        // 3 + 210 + 64 = 277 (with current RuleSet::full())
        assert_eq!(dec.size(), 277);
    }

    #[test]
    fn encoder_roundtrip() {
        let (enc, _) = make_vocabs();
        // Encode then decode should roundtrip
        let tokens = ["ADD", "MUL", "SIN", "V0", "I_1", "E", "FRAC_PI"];
        for tok in &tokens {
            let id = enc.encode(tok);
            assert_ne!(id, EncoderVocab::PAD, "token {} should not map to PAD", tok);
            assert_eq!(enc.decode(id), *tok);
        }
    }

    #[test]
    fn encoder_unknown_returns_pad() {
        let (enc, _) = make_vocabs();
        assert_eq!(enc.encode("UNKNOWN_TOKEN"), EncoderVocab::PAD);
    }

    #[test]
    fn decoder_encode_decode_roundtrip() {
        let (_, dec) = make_vocabs();
        // Special tokens
        assert_eq!(dec.decode(DecoderVocab::PAD), DecoderToken::Pad);
        assert_eq!(dec.decode(DecoderVocab::BOS), DecoderToken::Bos);
        assert_eq!(dec.decode(DecoderVocab::STOP), DecoderToken::Stop);

        // Rule tokens
        let rule_id = dec.encode_rule(5);
        assert_eq!(dec.decode(rule_id), DecoderToken::Rule(5));

        // Position tokens
        let pos_id = dec.encode_pos(10);
        assert_eq!(dec.decode(pos_id), DecoderToken::Pos(10));
    }

    #[test]
    fn decoder_pos_clamp() {
        let (_, dec) = make_vocabs();
        // Position >= max_positions should clamp to max_positions - 1
        let pos_id = dec.encode_pos(100);
        assert_eq!(dec.decode(pos_id), DecoderToken::Pos(63));
    }
}
