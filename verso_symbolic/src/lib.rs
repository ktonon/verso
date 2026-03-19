//! Public API for the symbolic core that powers parsing, simplification,
//! dimensional analysis, and formatting.
//!
//! The crate root is intended to be the primary integration surface:
//!
//! - parse expressions with [`parse_expr`]
//! - manipulate the expression tree with [`Expr`] and related constructors
//! - simplify or validate symbolic equalities with [`Context`] and search helpers
//! - render expressions with [`ToTex`] and text formatters

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
    format_dim_error, is_zero, subscript_base, Context, DimEnv, DimError, DimOutcome,
    EqualityResult,
};
pub use dim::{BaseDim, Dimension};
pub use eval::{eval_f64, free_vars, spot_check, SpotCheckFailure};
pub use expr::{
    add, acos, asin, atan, ceil, clamp, classify_mul, constant, cos, cosh, div, e_const, exp,
    frac_pi, floor, has_indices, infer_ty_from_kind, inv, ln, lower, match_log_base, max, min,
    mul, named, neg, pi, pow, quantity, rational, round, scalar, scalar_dim, sign, sin, sinh,
    sqrt, sub, tan, tanh, tensor, upper, Expr, ExprKind, FnKind, Index, IndexPosition, MulKind,
    NamedConst, Span, Ty,
};
pub use fmt::{fmt_colored, Colored};
pub use parser::{parse_expr, ParseError};
pub use rule::{
    const_wild, idx_exact, idx_lower, idx_upper, int_even_wild, int_odd_wild, int_wild, p_acos,
    p_add, p_asin, p_atan, p_ceil, p_clamp, p_const, p_cos, p_cosh, p_exp, p_floor, p_frac_pi,
    p_inv, p_ln, p_max, p_min, p_mul, p_named, p_neg, p_pow, p_rational, p_round, p_sign,
    p_sin, p_sinh, p_tan, p_tanh, p_var, p_var_wild, rule, rule_reversible, wildcard, Bindings,
    ExprBindings, IndexBindings, IndexPattern, Pattern, Rule, RuleSet, VarPattern,
};
pub use search::{
    eval_constants, simplify, simplify_with_trace, BeamSearch, SearchStrategy, TraceStep,
};
pub use to_tex::ToTex;
pub use unit::Unit;
