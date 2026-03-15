// Re-export from verso_symbolic — all dimensional analysis logic lives there.
pub use verso_symbolic::context::{
    check_claim_dim, collect_units, infer_dim,
    DimEnv, DimError, DimOutcome,
};
