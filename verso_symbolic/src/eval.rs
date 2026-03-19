use crate::expr::{Expr, ExprKind, FnKind};
use std::collections::{HashMap, HashSet};

/// Collect all free variable names from an expression.
pub fn free_vars(expr: &Expr) -> HashSet<String> {
    let mut vars = HashSet::new();
    expr.walk(&mut |e| {
        if let ExprKind::Var { name, .. } = &e.kind {
            vars.insert(name.clone());
        }
    });
    vars
}

/// Evaluate an expression to f64 given variable bindings.
/// Returns None if evaluation fails (e.g., unbound variable, division by zero).
pub fn eval_f64(expr: &Expr, bindings: &HashMap<String, f64>) -> Option<f64> {
    expr.try_fold_post_order(&mut |node, children: Vec<f64>| match (&node.kind, children.as_slice()) {
        (ExprKind::Rational(r), []) => Some(r.num() as f64 / r.den() as f64),
        (ExprKind::FracPi(r), []) => Some((r.num() as f64 / r.den() as f64) * std::f64::consts::PI),
        (ExprKind::Named(nc), []) => Some(nc.value()),
        (ExprKind::Var { name, .. }, []) => bindings.get(name).copied(),
        (ExprKind::Add(_, _), [a, b]) => Some(*a + *b),
        (ExprKind::Mul(_, _), [a, b]) => Some(*a * *b),
        (ExprKind::Neg(_), [value]) => Some(-*value),
        (ExprKind::Inv(_), [value]) => {
            if *value == 0.0 {
                None
            } else {
                Some(1.0 / *value)
            }
        }
        (ExprKind::Pow(_, _), [base, exp]) => {
            let result = base.powf(*exp);
            if result.is_finite() {
                Some(result)
            } else {
                None
            }
        }
        (ExprKind::Fn(kind, _), [value]) => {
            let result = match kind {
                FnKind::Sin => value.sin(),
                FnKind::Cos => value.cos(),
                FnKind::Tan => value.tan(),
                FnKind::Asin => value.asin(),
                FnKind::Acos => value.acos(),
                FnKind::Atan => value.atan(),
                FnKind::Sinh => value.sinh(),
                FnKind::Cosh => value.cosh(),
                FnKind::Tanh => value.tanh(),
                FnKind::Exp => value.exp(),
                FnKind::Ln => value.ln(),
                FnKind::Floor => value.floor(),
                FnKind::Ceil => value.ceil(),
                FnKind::Round => value.round(),
                FnKind::Sign => {
                    if *value > 0.0 {
                        1.0
                    } else if *value < 0.0 {
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
        (ExprKind::FnN(kind, _), values) => match kind {
            FnKind::Min if values.len() == 2 => Some(values[0].min(values[1])),
            FnKind::Max if values.len() == 2 => Some(values[0].max(values[1])),
            FnKind::Clamp if values.len() == 3 => Some(values[0].clamp(values[1], values[2])),
            _ => None,
        },
        (ExprKind::Quantity(_, unit), [value]) => Some(*value * unit.scale),
        _ => None,
    })
}

/// Numerically spot-check that two expressions are equal at random points.
/// Returns Ok(()) if all samples agree, Err with a counterexample if they disagree.
pub fn spot_check(lhs: &Expr, rhs: &Expr, num_samples: usize) -> Result<(), SpotCheckFailure> {
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
                &Expr::new(ExprKind::Fn(
                    FnKind::Sign,
                    Box::new(Expr::new(ExprKind::Rational(5.into())))
                )),
                &HashMap::new()
            ),
            Some(1.0)
        );
    }

    #[test]
    fn sign_negative() {
        assert_eq!(
            eval_f64(
                &Expr::new(ExprKind::Fn(
                    FnKind::Sign,
                    Box::new(Expr::new(ExprKind::Rational((-3).into())))
                )),
                &HashMap::new()
            ),
            Some(-1.0)
        );
    }

    #[test]
    fn sign_zero() {
        assert_eq!(
            eval_f64(
                &Expr::new(ExprKind::Fn(
                    FnKind::Sign,
                    Box::new(Expr::new(ExprKind::Rational(0.into())))
                )),
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
        let expr = Expr::new(ExprKind::Quantity(
            Box::new(Expr::new(ExprKind::Rational(3.into()))),
            unit,
        ));
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
