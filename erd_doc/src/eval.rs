use erd_symbolic::{Expr, FnKind};
use std::collections::{HashMap, HashSet};

/// Collect all free variable names from an expression.
pub fn free_vars(expr: &Expr) -> HashSet<String> {
    let mut vars = HashSet::new();
    collect_vars(expr, &mut vars);
    vars
}

fn collect_vars(expr: &Expr, vars: &mut HashSet<String>) {
    match expr {
        Expr::Var { name, .. } => {
            vars.insert(name.clone());
        }
        Expr::Add(a, b) | Expr::Mul(a, b) | Expr::Pow(a, b) => {
            collect_vars(a, vars);
            collect_vars(b, vars);
        }
        Expr::Neg(a) | Expr::Inv(a) | Expr::Fn(_, a) => {
            collect_vars(a, vars);
        }
        Expr::FnN(_, args) => {
            for a in args {
                collect_vars(a, vars);
            }
        }
        Expr::Rational(_) | Expr::FracPi(_) | Expr::Named(_) => {}
    }
}

/// Evaluate an expression to f64 given variable bindings.
/// Returns None if evaluation fails (e.g., unbound variable, division by zero).
pub fn eval_f64(expr: &Expr, bindings: &HashMap<String, f64>) -> Option<f64> {
    match expr {
        Expr::Rational(r) => Some(r.num() as f64 / r.den() as f64),
        Expr::FracPi(r) => Some((r.num() as f64 / r.den() as f64) * std::f64::consts::PI),
        Expr::Named(nc) => Some(nc.value()),
        Expr::Var { name, .. } => bindings.get(name).copied(),
        Expr::Add(a, b) => {
            let va = eval_f64(a, bindings)?;
            let vb = eval_f64(b, bindings)?;
            Some(va + vb)
        }
        Expr::Mul(a, b) => {
            let va = eval_f64(a, bindings)?;
            let vb = eval_f64(b, bindings)?;
            Some(va * vb)
        }
        Expr::Neg(a) => Some(-eval_f64(a, bindings)?),
        Expr::Inv(a) => {
            let v = eval_f64(a, bindings)?;
            if v == 0.0 {
                None
            } else {
                Some(1.0 / v)
            }
        }
        Expr::Pow(base, exp) => {
            let vb = eval_f64(base, bindings)?;
            let ve = eval_f64(exp, bindings)?;
            let result = vb.powf(ve);
            if result.is_finite() {
                Some(result)
            } else {
                None
            }
        }
        Expr::Fn(kind, arg) => {
            let v = eval_f64(arg, bindings)?;
            let result = match kind {
                FnKind::Sin => v.sin(),
                FnKind::Cos => v.cos(),
                FnKind::Tan => v.tan(),
                FnKind::Asin => v.asin(),
                FnKind::Acos => v.acos(),
                FnKind::Atan => v.atan(),
                FnKind::Sinh => v.sinh(),
                FnKind::Cosh => v.cosh(),
                FnKind::Tanh => v.tanh(),
                FnKind::Exp => v.exp(),
                FnKind::Ln => v.ln(),
                FnKind::Floor => v.floor(),
                FnKind::Ceil => v.ceil(),
                FnKind::Round => v.round(),
                FnKind::Sign => {
                    if v > 0.0 {
                        1.0
                    } else if v < 0.0 {
                        -1.0
                    } else {
                        0.0
                    }
                }
                _ => return None,
            };
            if result.is_finite() {
                Some(result)
            } else {
                None
            }
        }
        Expr::FnN(kind, args) => {
            let vals: Option<Vec<f64>> = args.iter().map(|a| eval_f64(a, bindings)).collect();
            let vals = vals?;
            match kind {
                FnKind::Min if vals.len() == 2 => Some(vals[0].min(vals[1])),
                FnKind::Max if vals.len() == 2 => Some(vals[0].max(vals[1])),
                FnKind::Clamp if vals.len() == 3 => Some(vals[0].clamp(vals[1], vals[2])),
                _ => None,
            }
        }
    }
}

/// Numerically spot-check that two expressions are equal at random points.
/// Returns Ok(()) if all samples agree, Err with a counterexample if they disagree.
pub fn spot_check(
    lhs: &Expr,
    rhs: &Expr,
    num_samples: usize,
) -> Result<(), SpotCheckFailure> {
    let mut all_vars = free_vars(lhs);
    all_vars.extend(free_vars(rhs));
    let var_names: Vec<String> = all_vars.into_iter().collect();

    // Use a simple deterministic sequence for reproducibility
    let mut seed: u64 = 0x1234_5678_9ABC_DEF0;

    for _ in 0..num_samples {
        let mut bindings = HashMap::new();
        for name in &var_names {
            // Generate values in a range that avoids singularities
            // Use range [-3, 3] to keep trig/power well-behaved
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let t = (seed >> 33) as f64 / (1u64 << 31) as f64; // [0, 1)
            let val = t * 6.0 - 3.0; // [-3, 3)
            bindings.insert(name.clone(), val);
        }

        let lhs_val = match eval_f64(lhs, &bindings) {
            Some(v) => v,
            None => continue, // skip points where evaluation fails
        };
        let rhs_val = match eval_f64(rhs, &bindings) {
            Some(v) => v,
            None => continue,
        };

        let diff = (lhs_val - rhs_val).abs();
        let scale = lhs_val.abs().max(rhs_val.abs()).max(1.0);
        let relative_err = diff / scale;

        if relative_err > 1e-8 {
            return Err(SpotCheckFailure {
                bindings,
                lhs_val,
                rhs_val,
            });
        }
    }

    Ok(())
}

#[derive(Debug)]
pub struct SpotCheckFailure {
    pub bindings: HashMap<String, f64>,
    pub lhs_val: f64,
    pub rhs_val: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use erd_symbolic::parse_expr;

    #[test]
    fn eval_constant() {
        let expr = parse_expr("42").unwrap();
        let val = eval_f64(&expr, &HashMap::new()).unwrap();
        assert!((val - 42.0).abs() < 1e-12);
    }

    #[test]
    fn eval_pi() {
        let expr = parse_expr("pi").unwrap();
        let val = eval_f64(&expr, &HashMap::new()).unwrap();
        assert!((val - std::f64::consts::PI).abs() < 1e-12);
    }

    #[test]
    fn eval_variable() {
        let expr = parse_expr("x + 1").unwrap();
        let mut bindings = HashMap::new();
        bindings.insert("x".to_string(), 5.0);
        let val = eval_f64(&expr, &bindings).unwrap();
        assert!((val - 6.0).abs() < 1e-12);
    }

    #[test]
    fn eval_trig() {
        let expr = parse_expr("sin(pi/2)").unwrap();
        let val = eval_f64(&expr, &HashMap::new()).unwrap();
        assert!((val - 1.0).abs() < 1e-12);
    }

    #[test]
    fn eval_unbound_var_returns_none() {
        let expr = parse_expr("x").unwrap();
        assert!(eval_f64(&expr, &HashMap::new()).is_none());
    }

    #[test]
    fn eval_division_by_zero_returns_none() {
        let expr = parse_expr("1/0").unwrap();
        // 1/0 parses as Mul(1, Inv(0)), Inv(0) should return None
        let val = eval_f64(&expr, &HashMap::new());
        assert!(val.is_none());
    }

    #[test]
    fn free_vars_collects_all() {
        let expr = parse_expr("x + y * sin(z)").unwrap();
        let vars = free_vars(&expr);
        assert!(vars.contains("x"));
        assert!(vars.contains("y"));
        assert!(vars.contains("z"));
        assert_eq!(vars.len(), 3);
    }

    #[test]
    fn spot_check_identity_passes() {
        let lhs = parse_expr("sin(x)^2 + cos(x)^2").unwrap();
        let rhs = parse_expr("1").unwrap();
        assert!(spot_check(&lhs, &rhs, 100).is_ok());
    }

    #[test]
    fn spot_check_wrong_identity_fails() {
        let lhs = parse_expr("x + 1").unwrap();
        let rhs = parse_expr("x").unwrap();
        assert!(spot_check(&lhs, &rhs, 100).is_err());
    }

    #[test]
    fn spot_check_distributive() {
        let lhs = parse_expr("(x + y)(x - y)").unwrap();
        let rhs = parse_expr("x^2 - y^2").unwrap();
        assert!(spot_check(&lhs, &rhs, 100).is_ok());
    }
}
