// Re-export from ogma_symbolic — all dimensional analysis logic lives there.
pub use ogma_symbolic::context::{
    check_claim_dim, check_dim, collect_units, DimEnv, DimError, DimOutcome,
};
