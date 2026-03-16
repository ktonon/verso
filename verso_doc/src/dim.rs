// Re-export from verso_symbolic — all dimensional analysis logic lives there.
pub use verso_symbolic::context::{
    check_claim_dim, check_dim, collect_units, DimEnv, DimError, DimOutcome,
};
