use crate::expr::{Expr, FnKind};
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
        Expr::Quantity(inner, _) => {
            collect_vars(inner, vars);
        }
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
        Expr::Quantity(inner, unit) => {
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
