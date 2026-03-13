use erd_symbolic::Expr;
use std::collections::{BTreeMap, HashMap};
use std::fmt;

/// Base physical dimensions (SI).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BaseDim {
    L,     // Length
    M,     // Mass
    T,     // Time
    Theta, // Temperature
    I,     // Electric current
    N,     // Amount of substance
    J,     // Luminous intensity
}

impl BaseDim {
    fn from_str(s: &str) -> Option<BaseDim> {
        match s {
            "L" => Some(BaseDim::L),
            "M" => Some(BaseDim::M),
            "T" => Some(BaseDim::T),
            "Θ" | "Theta" => Some(BaseDim::Theta),
            "I" => Some(BaseDim::I),
            "N" => Some(BaseDim::N),
            "J" => Some(BaseDim::J),
            _ => None,
        }
    }
}

impl fmt::Display for BaseDim {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BaseDim::L => write!(f, "L"),
            BaseDim::M => write!(f, "M"),
            BaseDim::T => write!(f, "T"),
            BaseDim::Theta => write!(f, "Θ"),
            BaseDim::I => write!(f, "I"),
            BaseDim::N => write!(f, "N"),
            BaseDim::J => write!(f, "J"),
        }
    }
}

/// A physical dimension as a product of base dimensions with integer exponents.
/// E.g., force = M L T^-2 is represented as {M: 1, L: 1, T: -2}.
/// Dimensionless = empty map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dimension {
    exponents: BTreeMap<BaseDim, i32>,
}

impl Dimension {
    pub fn dimensionless() -> Self {
        Dimension {
            exponents: BTreeMap::new(),
        }
    }

    pub fn is_dimensionless(&self) -> bool {
        self.exponents.values().all(|&e| e == 0)
    }

    /// Multiply dimensions (add exponents).
    pub fn mul(&self, other: &Dimension) -> Dimension {
        let mut result = self.exponents.clone();
        for (&base, &exp) in &other.exponents {
            *result.entry(base).or_insert(0) += exp;
        }
        result.retain(|_, e| *e != 0);
        Dimension { exponents: result }
    }

    /// Inverse dimension (negate exponents).
    pub fn inv(&self) -> Dimension {
        let exponents = self.exponents.iter().map(|(&b, &e)| (b, -e)).collect();
        Dimension { exponents }
    }

    /// Raise to integer power (multiply all exponents).
    pub fn pow(&self, n: i32) -> Dimension {
        if n == 0 {
            return Dimension::dimensionless();
        }
        let exponents = self
            .exponents
            .iter()
            .map(|(&b, &e)| (b, e * n))
            .filter(|(_, e)| *e != 0)
            .collect();
        Dimension { exponents }
    }

    /// Parse a dimension from bracket notation: `[M L T^-2]`
    pub fn parse(s: &str) -> Result<Dimension, String> {
        let s = s.trim();
        let s = s
            .strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
            .ok_or_else(|| {
                "dimension must be enclosed in brackets, e.g. [M L T^-2]".to_string()
            })?;
        let s = s.trim();

        if s == "1" || s.is_empty() {
            return Ok(Dimension::dimensionless());
        }

        let mut exponents = BTreeMap::new();
        for token in s.split_whitespace() {
            let (base_str, exp) = if let Some(caret_pos) = token.find('^') {
                let base_str = &token[..caret_pos];
                let exp_str = &token[caret_pos + 1..];
                let exp: i32 = exp_str
                    .parse()
                    .map_err(|_| format!("invalid exponent '{}' in dimension", exp_str))?;
                (base_str, exp)
            } else {
                (token, 1)
            };

            let base = BaseDim::from_str(base_str)
                .ok_or_else(|| format!("unknown base dimension '{}'", base_str))?;

            *exponents.entry(base).or_insert(0) += exp;
        }

        exponents.retain(|_, e| *e != 0);
        Ok(Dimension { exponents })
    }
}

impl fmt::Display for Dimension {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_dimensionless() {
            return write!(f, "[1]");
        }
        write!(f, "[")?;
        let mut first = true;
        for (base, exp) in &self.exponents {
            if !first {
                write!(f, " ")?;
            }
            first = false;
            if *exp == 1 {
                write!(f, "{}", base)?;
            } else {
                write!(f, "{}^{}", base, exp)?;
            }
        }
        write!(f, "]")
    }
}

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
        Expr::Var { name, .. } => env
            .get(name)
            .cloned()
            .ok_or_else(|| DimError::UndeclaredVar(name.clone())),
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
        Expr::Var { name, .. } => {
            if !env.contains_key(name) {
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
    }
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
        let expr = parse_expr("v**2").unwrap();
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
        let expr = parse_expr("(1/2) * m * v**2").unwrap();
        assert_eq!(infer_dim(&expr, &env).unwrap(), dim("[M L^2 T^-2]"));
    }
}
