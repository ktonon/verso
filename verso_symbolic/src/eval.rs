use crate::expr::{Expr, ExprKind, FnKind};
use std::collections::{HashMap, HashSet};

/// Collect all free variable names from an expression.
pub fn free_vars(expr: &Expr) -> HashSet<String> {
    let mut vars = HashSet::new();
    collect_vars(expr, &mut vars);
    vars
}

fn collect_vars(expr: &Expr, vars: &mut HashSet<String>) {
    match &expr.kind {
        ExprKind::Var { name, .. } => {
            vars.insert(name.clone());
        }
        ExprKind::Add(a, b) | ExprKind::Mul(a, b) | ExprKind::Pow(a, b) => {
            collect_vars(a, vars);
            collect_vars(b, vars);
        }
        ExprKind::Neg(a) | ExprKind::Inv(a) | ExprKind::Fn(_, a) => {
            collect_vars(a, vars);
        }
        ExprKind::FnN(_, args) => {
            for a in args {
                collect_vars(a, vars);
            }
        }
        ExprKind::Rational(_) | ExprKind::FracPi(_) | ExprKind::Named(_) => {}
        ExprKind::Quantity(inner, _) => {
            collect_vars(inner, vars);
        }
    }
}

/// Evaluate an expression to f64 given variable bindings.
/// Returns None if evaluation fails (e.g., unbound variable, division by zero).
pub fn eval_f64(expr: &Expr, bindings: &HashMap<String, f64>) -> Option<f64> {
    match &expr.kind {
        ExprKind::Rational(r) => Some(r.num() as f64 / r.den() as f64),
        ExprKind::FracPi(r) => Some((r.num() as f64 / r.den() as f64) * std::f64::consts::PI),
        ExprKind::Named(nc) => Some(nc.value()),
        ExprKind::Var { name, .. } => bindings.get(name).copied(),
        ExprKind::Add(a, b) => {
            let va = eval_f64(a, bindings)?;
            let vb = eval_f64(b, bindings)?;
            Some(va + vb)
        }
        ExprKind::Mul(a, b) => {
            let va = eval_f64(a, bindings)?;
            let vb = eval_f64(b, bindings)?;
            Some(va * vb)
        }
        ExprKind::Neg(a) => Some(-eval_f64(a, bindings)?),
        ExprKind::Inv(a) => {
            let v = eval_f64(a, bindings)?;
            if v == 0.0 {
                None
            } else {
                Some(1.0 / v)
            }
        }
        ExprKind::Pow(base, exp) => {
            let vb = eval_f64(base, bindings)?;
            let ve = eval_f64(exp, bindings)?;
            let result = vb.powf(ve);
            if result.is_finite() {
                Some(result)
            } else {
                None
            }
        }
        ExprKind::Fn(kind, arg) => {
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
        ExprKind::FnN(kind, args) => {
            let vals: Option<Vec<f64>> = args.iter().map(|a| eval_f64(a, bindings)).collect();
            let vals = vals?;
            match kind {
                FnKind::Min if vals.len() == 2 => Some(vals[0].min(vals[1])),
                FnKind::Max if vals.len() == 2 => Some(vals[0].max(vals[1])),
                FnKind::Clamp if vals.len() == 3 => Some(vals[0].clamp(vals[1], vals[2])),
                _ => None,
            }
        }
        ExprKind::Quantity(inner, unit) => {
            let v = eval_f64(inner, bindings)?;
            Some(v * unit.scale)
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

    let mut seed: u64 = 0x1234_5678_9ABC_DEF0;

    for _ in 0..num_samples {
        let mut bindings = HashMap::new();
        for name in &var_names {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let t = (seed >> 33) as f64 / (1u64 << 31) as f64;
            let val = t * 6.0 - 3.0;
            bindings.insert(name.clone(), val);
        }

        let lhs_val = match eval_f64(lhs, &bindings) {
            Some(v) => v,
            None => continue,
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
    use crate::expr::ExprKind;
    use crate::parser::parse_expr;
    use crate::unit::Unit;

    fn eval(s: &str) -> Option<f64> {
        let expr = parse_expr(s).unwrap();
        eval_f64(&expr, &HashMap::new())
    }

    fn eval_with(s: &str, bindings: &[(&str, f64)]) -> Option<f64> {
        let expr = parse_expr(s).unwrap();
        let b: HashMap<String, f64> = bindings.iter().map(|(k, v)| (k.to_string(), *v)).collect();
        eval_f64(&expr, &b)
    }

    #[test]
    fn sign_positive() {
        assert_eq!(
            eval_f64(
                &Expr::new(ExprKind::Fn(FnKind::Sign, Box::new(Expr::new(ExprKind::Rational(5.into()))))),
                &HashMap::new()
            ),
            Some(1.0)
        );
    }

    #[test]
    fn sign_negative() {
        assert_eq!(
            eval_f64(
                &Expr::new(ExprKind::Fn(FnKind::Sign, Box::new(Expr::new(ExprKind::Rational((-3).into()))))),
                &HashMap::new()
            ),
            Some(-1.0)
        );
    }

    #[test]
    fn sign_zero() {
        assert_eq!(
            eval_f64(
                &Expr::new(ExprKind::Fn(FnKind::Sign, Box::new(Expr::new(ExprKind::Rational(0.into()))))),
                &HashMap::new()
            ),
            Some(0.0)
        );
    }

    #[test]
    fn custom_fn_returns_none() {
        let expr = Expr::new(ExprKind::Fn(
            FnKind::Custom("foo".to_string()),
            Box::new(Expr::new(ExprKind::Rational(1.into()))),
        ));
        assert_eq!(eval_f64(&expr, &HashMap::new()), None);
    }

    #[test]
    fn inv_zero_returns_none() {
        assert_eq!(eval("1/0"), None);
    }

    #[test]
    fn pow_non_finite_returns_none() {
        // (-1)^0.5 = NaN
        assert_eq!(eval_with("x^y", &[("x", -1.0), ("y", 0.5)]), None);
    }

    #[test]
    fn fn_non_finite_returns_none() {
        // ln(0) = -inf
        assert_eq!(eval("ln(0)"), None);
    }

    #[test]
    fn quantity_applies_unit_scale() {
        use crate::dim::{BaseDim, Dimension};
        let unit = Unit {
            dimension: Dimension::single(BaseDim::L, 1),
            scale: 1000.0,
            display: "km".to_string(),
        };
        let expr = Expr::new(ExprKind::Quantity(Box::new(Expr::new(ExprKind::Rational(3.into()))), unit));
        assert_eq!(eval_f64(&expr, &HashMap::new()), Some(3000.0));
    }

    #[test]
    fn fnn_wrong_arity_returns_none() {
        // min with 3 args
        let expr = Expr::new(ExprKind::FnN(
            FnKind::Min,
            vec![
                Expr::new(ExprKind::Rational(1.into())),
                Expr::new(ExprKind::Rational(2.into())),
                Expr::new(ExprKind::Rational(3.into())),
            ],
        ));
        assert_eq!(eval_f64(&expr, &HashMap::new()), None);
    }

    #[test]
    fn unbound_var_returns_none() {
        assert_eq!(eval_with("x + 1", &[]), None);
    }

    #[test]
    fn spot_check_equal() {
        let lhs = parse_expr("x + 1").unwrap();
        let rhs = parse_expr("1 + x").unwrap();
        assert!(spot_check(&lhs, &rhs, 100).is_ok());
    }

    #[test]
    fn spot_check_unequal() {
        let lhs = parse_expr("x").unwrap();
        let rhs = parse_expr("x + 1").unwrap();
        assert!(spot_check(&lhs, &rhs, 100).is_err());
    }
}
