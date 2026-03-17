pub mod context;
pub mod dim;
pub mod eval;
mod expr;
pub mod fmt;
pub mod gen_expr;
mod parser;
pub mod random_search;
pub mod rational;
pub mod repl;
mod rule;
mod search;
mod to_tex;
pub mod token;
pub mod training_data;
pub mod unicode;
pub mod unit;
pub mod validate;

pub use context::{
    format_dim_error, is_zero, Context, DimEnv, DimError, DimOutcome, EqualityResult,
};
pub use dim::{BaseDim, Dimension};
pub use eval::{eval_f64, free_vars, spot_check, SpotCheckFailure};
pub use expr::*;
pub use fmt::{fmt_colored, Colored};
pub use parser::{parse_expr, ParseError};
pub use rule::*;
pub use search::*;
pub use to_tex::ToTex;
pub use unit::Unit;
