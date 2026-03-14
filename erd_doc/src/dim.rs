use erd_symbolic::{Dimension, Expr};
use std::collections::HashMap;
use std::fmt;

/// Map from variable name to its declared dimension.
pub type DimEnv = HashMap<String, Dimension>;

/// A dimensional analysis error.
#[derive(Debug)]
pub enum DimError {
    UndeclaredVar(String),
    Mismatch {
        expected: Dimension,
        got: Dimension,
        context: String,
    },
    NonDimensionlessFnArg {
        func: String,
        dim: Dimension,
    },
    NonIntegerPower,
}

impl fmt::Display for DimError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DimError::UndeclaredVar(v) => write!(f, "variable '{}' has no :dim declaration", v),
            DimError::Mismatch {
                expected,
                got,
                context,
            } => write!(
                f,
                "dimension mismatch in {}: expected {}, got {}",
                context, expected, got
            ),
            DimError::NonDimensionlessFnArg { func, dim } => {
                write!(
                    f,
                    "argument to {}() must be dimensionless, got {}",
                    func, dim
                )
            }
            DimError::NonIntegerPower => {
                write!(f, "cannot raise dimensional quantity to non-integer power")
            }
        }
    }
}

/// Result of dimension checking a claim.
#[derive(Debug)]
pub enum DimOutcome {
    /// All dimensions consistent.
    Pass,
    /// Some variables lack :dim declarations — check skipped.
    Skipped { undeclared: Vec<String> },
    /// Dimension mismatch between lhs and rhs.
    LhsRhsMismatch { lhs: Dimension, rhs: Dimension },
    /// Error within an expression (e.g., adding length to time).
    ExprError { side: String, error: DimError },
}

impl DimOutcome {
    pub fn passed(&self) -> bool {
        matches!(self, DimOutcome::Pass | DimOutcome::Skipped { .. })
    }
}

/// Infer the dimension of an expression given a dimension environment.
pub fn infer_dim(expr: &Expr, env: &DimEnv) -> Result<Dimension, DimError> {
    match expr {
        Expr::Rational(_) | Expr::FracPi(_) | Expr::Named(_) => Ok(Dimension::dimensionless()),
        Expr::Quantity(_inner, unit) => Ok(unit.dimension.clone()),
        Expr::Var { name, dim, .. } => {
            // Inline dimension annotation takes precedence over environment
            if let Some(d) = dim {
                return Ok(d.clone());
            }
            env.get(name)
                .cloned()
                .ok_or_else(|| DimError::UndeclaredVar(name.clone()))
        }
        Expr::Add(a, b) => {
            let da = infer_dim(a, env)?;
            let db = infer_dim(b, env)?;
            if da != db {
                return Err(DimError::Mismatch {
                    expected: da,
                    got: db,
                    context: "addition".to_string(),
                });
            }
            Ok(da)
        }
        Expr::Mul(a, b) => {
            let da = infer_dim(a, env)?;
            let db = infer_dim(b, env)?;
            Ok(da.mul(&db))
        }
        Expr::Neg(inner) => infer_dim(inner, env),
        Expr::Inv(inner) => {
            let d = infer_dim(inner, env)?;
            Ok(d.inv())
        }
        Expr::Pow(base, exp) => {
            let db = infer_dim(base, env)?;
            if db.is_dimensionless() {
                // dimensionless^anything = dimensionless, but exponent must also be dimensionless
                let de = infer_dim(exp, env)?;
                if !de.is_dimensionless() {
                    return Err(DimError::Mismatch {
                        expected: Dimension::dimensionless(),
                        got: de,
                        context: "exponent".to_string(),
                    });
                }
                return Ok(Dimension::dimensionless());
            }
            // Dimensional base: exponent must be an integer constant
            let n = expr_as_integer(exp).ok_or(DimError::NonIntegerPower)?;
            Ok(db.pow(n))
        }
        Expr::Fn(kind, _arg) => {
            let da = infer_dim(_arg, env)?;
            if !da.is_dimensionless() {
                return Err(DimError::NonDimensionlessFnArg {
                    func: format!("{:?}", kind).to_lowercase(),
                    dim: da,
                });
            }
            Ok(Dimension::dimensionless())
        }
        Expr::FnN(kind, args) => {
            for arg in args {
                let da = infer_dim(arg, env)?;
                if !da.is_dimensionless() {
                    return Err(DimError::NonDimensionlessFnArg {
                        func: format!("{:?}", kind).to_lowercase(),
                        dim: da,
                    });
                }
            }
            Ok(Dimension::dimensionless())
        }
    }
}

/// Check that two sides of a claim have the same dimension.
pub fn check_claim_dim(
    lhs: &Expr,
    rhs: &Expr,
    env: &DimEnv,
) -> DimOutcome {
    let dl = match infer_dim(lhs, env) {
        Ok(d) => d,
        Err(DimError::UndeclaredVar(v)) => {
            // Collect all undeclared vars
            let mut undeclared = collect_undeclared(lhs, env);
            undeclared.extend(collect_undeclared(rhs, env));
            undeclared.sort();
            undeclared.dedup();
            if undeclared.is_empty() {
                undeclared.push(v);
            }
            return DimOutcome::Skipped { undeclared };
        }
        Err(e) => {
            return DimOutcome::ExprError {
                side: "lhs".to_string(),
                error: e,
            }
        }
    };

    let dr = match infer_dim(rhs, env) {
        Ok(d) => d,
        Err(DimError::UndeclaredVar(v)) => {
            let mut undeclared = collect_undeclared(rhs, env);
            if undeclared.is_empty() {
                undeclared.push(v);
            }
            return DimOutcome::Skipped { undeclared };
        }
        Err(e) => {
            return DimOutcome::ExprError {
                side: "rhs".to_string(),
                error: e,
            }
        }
    };

    if dl == dr {
        DimOutcome::Pass
    } else {
        DimOutcome::LhsRhsMismatch { lhs: dl, rhs: dr }
    }
}

/// Collect all variable names in an expression that lack dimension declarations.
fn collect_undeclared(expr: &Expr, env: &DimEnv) -> Vec<String> {
    let mut undeclared = Vec::new();
    collect_undeclared_inner(expr, env, &mut undeclared);
    undeclared.sort();
    undeclared.dedup();
    undeclared
}

fn collect_undeclared_inner(expr: &Expr, env: &DimEnv, out: &mut Vec<String>) {
    match expr {
        Expr::Var { name, dim, .. } => {
            if dim.is_none() && !env.contains_key(name) {
                out.push(name.clone());
            }
        }
        Expr::Add(a, b) | Expr::Mul(a, b) | Expr::Pow(a, b) => {
            collect_undeclared_inner(a, env, out);
            collect_undeclared_inner(b, env, out);
        }
        Expr::Neg(inner) | Expr::Inv(inner) | Expr::Fn(_, inner) => {
            collect_undeclared_inner(inner, env, out);
        }
        Expr::FnN(_, args) => {
            for arg in args {
                collect_undeclared_inner(arg, env, out);
            }
        }
        Expr::Rational(_) | Expr::FracPi(_) | Expr::Named(_) => {}
        Expr::Quantity(inner, _) => {
            collect_undeclared_inner(inner, env, out);
        }
    }
}

/// Collect all unit display names from Quantity nodes in an expression.
pub fn collect_units(expr: &Expr) -> Vec<String> {
    expr.collect_units()
}

/// Try to extract an integer value from an expression.
fn expr_as_integer(expr: &Expr) -> Option<i32> {
    match expr {
        Expr::Rational(r) => {
            if r.den() == 1 {
                Some(r.num() as i32)
            } else {
                None
            }
        }
        Expr::Neg(inner) => expr_as_integer(inner).map(|n| -n),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erd_symbolic::parse_expr;

    fn dim(s: &str) -> Dimension {
        Dimension::parse(s).unwrap()
    }

    fn env_from(pairs: &[(&str, &str)]) -> DimEnv {
        pairs
            .iter()
            .map(|(name, d)| (name.to_string(), dim(d)))
            .collect()
    }

    #[test]
    fn parse_simple_dimension() {
        let d = dim("[L]");
        assert_eq!(d.to_string(), "[L]");
    }

    #[test]
    fn parse_compound_dimension() {
        let d = dim("[M L T^-2]");
        assert_eq!(d.to_string(), "[L M T^-2]"); // BTreeMap sorts by key
    }

    #[test]
    fn parse_dimensionless() {
        assert!(dim("[1]").is_dimensionless());
    }

    #[test]
    fn dimension_mul() {
        let mass = dim("[M]");
        let accel = dim("[L T^-2]");
        let force = mass.mul(&accel);
        assert_eq!(force, dim("[M L T^-2]"));
    }

    #[test]
    fn dimension_inv() {
        let time = dim("[T]");
        let freq = time.inv();
        assert_eq!(freq, dim("[T^-1]"));
    }

    #[test]
    fn dimension_pow() {
        let length = dim("[L]");
        let area = length.pow(2);
        assert_eq!(area, dim("[L^2]"));
    }

    #[test]
    fn infer_var_dimension() {
        let env = env_from(&[("x", "[L]")]);
        let expr = parse_expr("x").unwrap();
        assert_eq!(infer_dim(&expr, &env).unwrap(), dim("[L]"));
    }

    #[test]
    fn infer_product_dimension() {
        let env = env_from(&[("m", "[M]"), ("a", "[L T^-2]")]);
        let expr = parse_expr("m * a").unwrap();
        assert_eq!(infer_dim(&expr, &env).unwrap(), dim("[M L T^-2]"));
    }

    #[test]
    fn infer_division_dimension() {
        let env = env_from(&[("x", "[L]"), ("t", "[T]")]);
        let expr = parse_expr("x / t").unwrap();
        assert_eq!(infer_dim(&expr, &env).unwrap(), dim("[L T^-1]"));
    }

    #[test]
    fn infer_power_dimension() {
        let env = env_from(&[("v", "[L T^-1]")]);
        let expr = parse_expr("v^2").unwrap();
        assert_eq!(infer_dim(&expr, &env).unwrap(), dim("[L^2 T^-2]"));
    }

    #[test]
    fn addition_dimension_mismatch() {
        let env = env_from(&[("x", "[L]"), ("t", "[T]")]);
        let expr = parse_expr("x + t").unwrap();
        assert!(matches!(
            infer_dim(&expr, &env),
            Err(DimError::Mismatch { .. })
        ));
    }

    #[test]
    fn function_requires_dimensionless_arg() {
        let env = env_from(&[("x", "[L]")]);
        let expr = parse_expr("sin(x)").unwrap();
        assert!(matches!(
            infer_dim(&expr, &env),
            Err(DimError::NonDimensionlessFnArg { .. })
        ));
    }

    #[test]
    fn dimensionless_function_ok() {
        let env = env_from(&[("theta", "[1]")]);
        let expr = parse_expr("sin(theta)").unwrap();
        assert_eq!(infer_dim(&expr, &env).unwrap(), Dimension::dimensionless());
    }

    #[test]
    fn check_claim_pass() {
        let env = env_from(&[("F", "[M L T^-2]"), ("m", "[M]"), ("a", "[L T^-2]")]);
        let lhs = parse_expr("F").unwrap();
        let rhs = parse_expr("m * a").unwrap();
        assert!(check_claim_dim(&lhs, &rhs, &env).passed());
    }

    #[test]
    fn check_claim_fail() {
        let env = env_from(&[("x", "[L]"), ("t", "[T]")]);
        let lhs = parse_expr("x").unwrap();
        let rhs = parse_expr("t").unwrap();
        assert!(!check_claim_dim(&lhs, &rhs, &env).passed());
    }

    #[test]
    fn check_claim_skipped_undeclared() {
        let env = env_from(&[("x", "[L]")]);
        let lhs = parse_expr("x").unwrap();
        let rhs = parse_expr("y").unwrap();
        match check_claim_dim(&lhs, &rhs, &env) {
            DimOutcome::Skipped { undeclared } => {
                assert_eq!(undeclared, vec!["y"]);
            }
            other => panic!("expected Skipped, got {:?}", other),
        }
    }

    #[test]
    fn constant_is_dimensionless() {
        let env: DimEnv = HashMap::new();
        let expr = parse_expr("42").unwrap();
        assert_eq!(infer_dim(&expr, &env).unwrap(), Dimension::dimensionless());
    }

    #[test]
    fn kinetic_energy_dimensions() {
        let env = env_from(&[("m", "[M]"), ("v", "[L T^-1]")]);
        // (1/2) m v^2 has dimension [M L^2 T^-2] = energy
        let expr = parse_expr("(1/2) * m * v^2").unwrap();
        assert_eq!(infer_dim(&expr, &env).unwrap(), dim("[M L^2 T^-2]"));
    }

    #[test]
    fn inline_dimension_annotation() {
        let env: DimEnv = HashMap::new();
        let expr = parse_expr("v [L T^-1]").unwrap();
        assert_eq!(infer_dim(&expr, &env).unwrap(), dim("[L T^-1]"));
    }

    #[test]
    fn inline_dim_overrides_env() {
        let env = env_from(&[("v", "[M]")]); // wrong dim in env
        let expr = parse_expr("v [L T^-1]").unwrap();
        // Inline annotation takes precedence
        assert_eq!(infer_dim(&expr, &env).unwrap(), dim("[L T^-1]"));
    }

    #[test]
    fn quantity_has_unit_dimension() {
        let env: DimEnv = HashMap::new();
        // 5 [m] has dimension [L]
        let expr = parse_expr("5 [m]").unwrap();
        assert_eq!(infer_dim(&expr, &env).unwrap(), dim("[L]"));
    }

    #[test]
    fn quantity_derived_unit_dimension() {
        let env: DimEnv = HashMap::new();
        // 100 [N] has dimension [M L T^-2]
        let expr = parse_expr("100 [N]").unwrap();
        assert_eq!(infer_dim(&expr, &env).unwrap(), dim("[M L T^-2]"));
    }

    #[test]
    fn quantity_compound_unit_dimension() {
        let env: DimEnv = HashMap::new();
        // 3*10^8 [m/s] has dimension [L T^-1]
        let expr = parse_expr("3*10^8 [m/s]").unwrap();
        assert_eq!(infer_dim(&expr, &env).unwrap(), dim("[L T^-1]"));
    }

    #[test]
    fn check_claim_quantity_vs_var() {
        // F = 10 [N] should pass when F has dim [M L T^-2]
        let env = env_from(&[("F", "[M L T^-2]")]);
        let lhs = parse_expr("F").unwrap();
        let rhs = parse_expr("10 [N]").unwrap();
        assert!(check_claim_dim(&lhs, &rhs, &env).passed());
    }

    #[test]
    fn collect_units_from_quantity() {
        let expr = parse_expr("1000 [m]").unwrap();
        let units = collect_units(&expr);
        assert_eq!(units, vec!["m"]);
    }

    #[test]
    fn collect_units_from_both_sides() {
        let lhs = parse_expr("1000 [m]").unwrap();
        let rhs = parse_expr("1 [km]").unwrap();
        let mut units = collect_units(&lhs);
        units.extend(collect_units(&rhs));
        units.sort();
        units.dedup();
        assert_eq!(units, vec!["km", "m"]);
    }

    #[test]
    fn collect_units_empty_for_pure_expr() {
        let expr = parse_expr("x + y").unwrap();
        let units = collect_units(&expr);
        assert!(units.is_empty());
    }

    #[test]
    fn collect_units_compound() {
        let expr = parse_expr("3*10^8 [m/s]").unwrap();
        let units = collect_units(&expr);
        assert_eq!(units, vec!["m/s"]);
    }

    #[test]
    fn check_claim_quantity_dim_mismatch() {
        // x [L] = 5 [s] should fail (length vs time)
        let env: DimEnv = HashMap::new();
        let lhs = parse_expr("5 [m]").unwrap();
        let rhs = parse_expr("3 [s]").unwrap();
        assert!(!check_claim_dim(&lhs, &rhs, &env).passed());
    }
}
