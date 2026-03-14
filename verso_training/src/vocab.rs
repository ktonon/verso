use verso_symbolic::random_search::IndexedRuleSet;
use verso_symbolic::training_data::build_vocab_metadata;

/// Encoder vocabulary: maps input token strings to integer IDs.
///
/// Layout: [PAD=0, token_0=1, token_1=2, ...]
/// Built directly from `verso_symbolic` types — no vocab.json needed.
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

#[cfg(test)]
mod tests {
    use super::*;
    use verso_symbolic::RuleSet;

    fn make_vocab() -> EncoderVocab {
        let indexed = IndexedRuleSet::new(RuleSet::full());
        EncoderVocab::new(&indexed)
    }

    #[test]
    fn encoder_vocab_size_matches_python() {
        let enc = make_vocab();
        // 75 tokens + PAD = 76
        assert_eq!(enc.size(), 76);
    }

    #[test]
    fn encoder_roundtrip() {
        let enc = make_vocab();
        let tokens = ["ADD", "MUL", "SIN", "V0", "I_1", "E", "FRAC_PI"];
        for tok in &tokens {
            let id = enc.encode(tok);
            assert_ne!(id, EncoderVocab::PAD, "token {} should not map to PAD", tok);
            assert_eq!(enc.decode(id), *tok);
        }
    }

    #[test]
    fn encoder_unknown_returns_pad() {
        let enc = make_vocab();
        assert_eq!(enc.encode("UNKNOWN_TOKEN"), EncoderVocab::PAD);
    }
}
