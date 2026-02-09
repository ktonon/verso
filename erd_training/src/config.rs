/// Configuration for the single-step policy model (encoder-only).
#[derive(Debug, Clone)]
pub struct PolicyConfig {
    pub d_model: usize,
    pub n_encoder_layers: usize,
    pub n_heads: usize,
    pub d_ff: usize,
    pub dropout: f64,
    pub max_enc_len: usize,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            d_model: 128,
            n_encoder_layers: 4,
            n_heads: 4,
            d_ff: 256,
            dropout: 0.1,
            max_enc_len: 64,
        }
    }
}
