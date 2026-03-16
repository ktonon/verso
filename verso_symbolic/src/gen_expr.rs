use crate::expr::{Expr, ExprKind, FnKind, NamedConst};
use crate::rational::Rational;
use rand::distr::weighted::WeightedIndex;
use rand::prelude::Distribution;
use rand::prelude::IndexedRandom;
use rand::rngs::StdRng;
use rand::Rng;

/// Configuration for random expression generation.
pub struct GenExprConfig {
    /// Maximum AST depth (default 5).
    pub max_depth: usize,
    /// Number of distinct variable names to use (default 3).
    pub num_vars: usize,
    /// Weight for Add nodes (default 3.0).
    pub weight_add: f64,
    /// Weight for Mul nodes (default 3.0).
    pub weight_mul: f64,
    /// Weight for Neg nodes (default 1.0).
    pub weight_neg: f64,
    /// Weight for Inv nodes (default 0.5).
    pub weight_inv: f64,
    /// Weight for Pow nodes (default 1.5).
    pub weight_pow: f64,
    /// Weight for Fn (trig/exp/ln) nodes (default 0.5).
    pub weight_fn: f64,
    /// Whether to include FracPi leaves (default false).
    pub include_frac_pi: bool,
    /// Whether to include named constants like e, sqrt2 (default false).
    pub include_named: bool,
}

impl Default for GenExprConfig {
    fn default() -> Self {
        GenExprConfig {
            max_depth: 5,
            num_vars: 3,
            weight_add: 3.0,
            weight_mul: 3.0,
            weight_neg: 1.0,
            weight_inv: 0.5,
            weight_pow: 1.5,
            weight_fn: 0.5,
            include_frac_pi: false,
            include_named: false,
        }
    }
}

const VAR_NAMES: &[&str] = &["x", "y", "z", "w", "u", "v"];
const SMALL_INTEGERS: &[i64] = &[-3, -2, -1, 1, 2, 3, 4, 5];
const SIMPLE_FRACTIONS: &[(i64, i64)] = &[(1, 2), (1, 3), (2, 3), (1, 4), (3, 4)];
const FRAC_PI_VALUES: &[(i64, i64)] = &[(1, 6), (1, 4), (1, 3), (1, 2), (1, 1), (2, 1)];
const SMALL_EXPONENTS: &[i64] = &[-2, -1, 2, 3];
const FN_POOL: &[FnKind] = &[FnKind::Sin, FnKind::Cos, FnKind::Tan, FnKind::Exp, FnKind::Ln];

/// Generate a random expression using the given RNG and config.
pub fn gen_expr(rng: &mut StdRng, config: &GenExprConfig) -> Expr {
    gen_expr_rec(rng, config, 0)
}

fn gen_expr_rec(rng: &mut StdRng, config: &GenExprConfig, depth: usize) -> Expr {
    // At max_depth, always generate a leaf
    if depth >= config.max_depth {
        return gen_leaf(rng, config);
    }

    // Leaf probability increases with depth
    let leaf_prob = depth as f64 / config.max_depth as f64;
    if depth > 0 && rng.random_bool(leaf_prob) {
        return gen_leaf(rng, config);
    }

    // Choose internal node type by weight
    let weights = [
        config.weight_add,
        config.weight_mul,
        config.weight_neg,
        config.weight_inv,
        config.weight_pow,
        config.weight_fn,
    ];
    let dist = WeightedIndex::new(&weights).expect("weights must be non-negative");
    let choice = dist.sample(rng);

    match choice {
        0 => {
            // Add
            let a = gen_expr_rec(rng, config, depth + 1);
            let b = gen_expr_rec(rng, config, depth + 1);
            Expr::new(ExprKind::Add(Box::new(a), Box::new(b)))
        }
        1 => {
            // Mul
            let a = gen_expr_rec(rng, config, depth + 1);
            let b = gen_expr_rec(rng, config, depth + 1);
            Expr::new(ExprKind::Mul(Box::new(a), Box::new(b)))
        }
        2 => {
            // Neg
            let a = gen_expr_rec(rng, config, depth + 1);
            Expr::new(ExprKind::Neg(Box::new(a)))
        }
        3 => {
            // Inv
            let a = gen_expr_rec(rng, config, depth + 1);
            Expr::new(ExprKind::Inv(Box::new(a)))
        }
        4 => {
            // Pow with small exponent
            let base = gen_expr_rec(rng, config, depth + 1);
            let exp_val = *SMALL_EXPONENTS.choose(rng).unwrap();
            Expr::new(ExprKind::Pow(
                Box::new(base),
                Box::new(Expr::new(ExprKind::Rational(Rational::from_i64(exp_val)))),
            ))
        }
        5 => {
            // Function (trig/exp/ln)
            let kind = FN_POOL.choose(rng).unwrap().clone();
            let arg = gen_expr_rec(rng, config, depth + 1);
            Expr::new(ExprKind::Fn(kind, Box::new(arg)))
        }
        _ => unreachable!(),
    }
}

fn gen_leaf(rng: &mut StdRng, config: &GenExprConfig) -> Expr {
    let mut weights = vec![5.0, 3.0, 1.0]; // Var, Int, Frac
    if config.include_frac_pi {
        weights.push(0.5);
    }
    if config.include_named {
        weights.push(0.5);
    }

    let dist = WeightedIndex::new(&weights).expect("weights must be non-negative");
    let choice = dist.sample(rng);

    match choice {
        0 => {
            // Variable
            let n = config.num_vars.min(VAR_NAMES.len());
            let name = VAR_NAMES[rng.random_range(0..n)];
            Expr::new(ExprKind::Var {
                name: name.to_string(),
                indices: vec![],
                dim: None,
            })
        }
        1 => {
            // Small integer
            let val = *SMALL_INTEGERS.choose(rng).unwrap();
            Expr::new(ExprKind::Rational(Rational::from_i64(val)))
        }
        2 => {
            // Simple fraction
            let (n, d) = *SIMPLE_FRACTIONS.choose(rng).unwrap();
            Expr::new(ExprKind::Rational(Rational::new(n, d)))
        }
        3 => {
            // FracPi
            let (n, d) = *FRAC_PI_VALUES.choose(rng).unwrap();
            Expr::new(ExprKind::FracPi(Rational::new(n, d)))
        }
        4 => {
            // Named constant
            let constants = [
                NamedConst::E,
                NamedConst::Sqrt2,
                NamedConst::Sqrt3,
                NamedConst::Frac1Sqrt2,
                NamedConst::FracSqrt3By2,
            ];
            Expr::new(ExprKind::Named(*constants.choose(rng).unwrap()))
        }
        _ => unreachable!(),
    }
}

/// Compute the depth of an expression tree.
pub fn expr_depth(expr: &Expr) -> usize {
    match &expr.kind {
        ExprKind::Rational(_) | ExprKind::Named(_) | ExprKind::FracPi(_) | ExprKind::Var { .. }
        | ExprKind::Quantity(_, _) => 0,
        ExprKind::Neg(a) | ExprKind::Inv(a) | ExprKind::Fn(_, a) => 1 + expr_depth(a),
        ExprKind::FnN(_, args) => 1 + args.iter().map(expr_depth).max().unwrap_or(0),
        ExprKind::Add(a, b) | ExprKind::Mul(a, b) | ExprKind::Pow(a, b) => {
            1 + expr_depth(a).max(expr_depth(b))
        }
    }
}

#[cfg(test)]
mod tests {
    fn collect_var_names(expr: &Expr, names: &mut Vec<String>) {
        match &expr.kind {
            ExprKind::Var { name, .. } => {
                if !names.contains(name) {
                    names.push(name.clone());
                }
            }
            ExprKind::Neg(a) | ExprKind::Inv(a) | ExprKind::Fn(_, a) => collect_var_names(a, names),
            ExprKind::FnN(_, args) => {
                for arg in args {
                    collect_var_names(arg, names);
                }
            }
            ExprKind::Add(a, b) | ExprKind::Mul(a, b) | ExprKind::Pow(a, b) => {
                collect_var_names(a, names);
                collect_var_names(b, names);
            }
            _ => {}
        }
    }

    fn has_frac_pi(expr: &Expr) -> bool {
        match &expr.kind {
            ExprKind::FracPi(_) => true,
            ExprKind::Neg(a) | ExprKind::Inv(a) | ExprKind::Fn(_, a) => has_frac_pi(a),
            ExprKind::FnN(_, args) => args.iter().any(has_frac_pi),
            ExprKind::Add(a, b) | ExprKind::Mul(a, b) | ExprKind::Pow(a, b) => {
                has_frac_pi(a) || has_frac_pi(b)
            }
            _ => false,
        }
    }

    fn has_node_type(expr: &Expr, check: &dyn Fn(&Expr) -> bool) -> bool {
        if check(expr) {
            return true;
        }
        match &expr.kind {
            ExprKind::Neg(a) | ExprKind::Inv(a) | ExprKind::Fn(_, a) => has_node_type(a, check),
            ExprKind::FnN(_, args) => args.iter().any(|a| has_node_type(a, check)),
            ExprKind::Add(a, b) | ExprKind::Mul(a, b) | ExprKind::Pow(a, b) => {
                has_node_type(a, check) || has_node_type(b, check)
            }
            _ => false,
        }
    }
    use super::*;
    use crate::token::{detokenize, tokenize};
    use rand::SeedableRng;

    #[test]
    fn deterministic_generation() {
        let config = GenExprConfig::default();
        let mut rng1 = StdRng::seed_from_u64(42);
        let mut rng2 = StdRng::seed_from_u64(42);
        let e1 = gen_expr(&mut rng1, &config);
        let e2 = gen_expr(&mut rng2, &config);
        assert_eq!(e1, e2);
    }

    #[test]
    fn respects_max_depth() {
        let config = GenExprConfig {
            max_depth: 4,
            ..Default::default()
        };
        for seed in 0..200 {
            let mut rng = StdRng::seed_from_u64(seed);
            let expr = gen_expr(&mut rng, &config);
            assert!(
                expr_depth(&expr) <= config.max_depth,
                "seed {} produced depth {} > max {}",
                seed,
                expr_depth(&expr),
                config.max_depth
            );
        }
    }

    #[test]
    fn uses_configured_variables() {
        let config = GenExprConfig {
            num_vars: 2,
            ..Default::default()
        };
        for seed in 0..200 {
            let mut rng = StdRng::seed_from_u64(seed);
            let expr = gen_expr(&mut rng, &config);
            let mut names = Vec::new();
            collect_var_names(&expr, &mut names);
            for name in &names {
                assert!(
                    name == "x" || name == "y",
                    "unexpected variable '{}' with num_vars=2",
                    name
                );
            }
        }
    }

    #[test]
    fn leaf_and_internal_types_present() {
        let config = GenExprConfig {
            max_depth: 5,
            ..Default::default()
        };
        let mut has_add = false;
        let mut has_mul = false;
        let mut has_neg = false;
        let mut has_inv = false;
        let mut has_pow = false;
        let mut has_fn = false;
        let mut has_var = false;
        let mut has_int = false;
        let mut has_frac = false;

        for seed in 0..1000 {
            let mut rng = StdRng::seed_from_u64(seed);
            let expr = gen_expr(&mut rng, &config);

            has_add |= has_node_type(&expr, &|e| matches!(e.kind, ExprKind::Add(_, _)));
            has_mul |= has_node_type(&expr, &|e| matches!(e.kind, ExprKind::Mul(_, _)));
            has_neg |= has_node_type(&expr, &|e| matches!(e.kind, ExprKind::Neg(_)));
            has_inv |= has_node_type(&expr, &|e| matches!(e.kind, ExprKind::Inv(_)));
            has_pow |= has_node_type(&expr, &|e| matches!(e.kind, ExprKind::Pow(_, _)));
            has_fn |= has_node_type(&expr, &|e| matches!(e.kind, ExprKind::Fn(_, _)));
            has_var |= has_node_type(&expr, &|e| matches!(e.kind, ExprKind::Var { .. }));
            has_int |=
                has_node_type(&expr, &|e| matches!(&e.kind, ExprKind::Rational(r) if r.is_integer()));
            has_frac |=
                has_node_type(&expr, &|e| matches!(&e.kind, ExprKind::Rational(r) if !r.is_integer()));
        }

        assert!(has_add, "no Add nodes in 1000 expressions");
        assert!(has_mul, "no Mul nodes in 1000 expressions");
        assert!(has_neg, "no Neg nodes in 1000 expressions");
        assert!(has_inv, "no Inv nodes in 1000 expressions");
        assert!(has_pow, "no Pow nodes in 1000 expressions");
        assert!(has_fn, "no Fn nodes in 1000 expressions");
        assert!(has_var, "no Var nodes in 1000 expressions");
        assert!(has_int, "no integer Rational nodes in 1000 expressions");
        assert!(has_frac, "no fraction Rational nodes in 1000 expressions");
    }

    #[test]
    fn no_frac_pi_when_disabled() {
        let config = GenExprConfig {
            include_frac_pi: false,
            ..Default::default()
        };
        for seed in 0..200 {
            let mut rng = StdRng::seed_from_u64(seed);
            let expr = gen_expr(&mut rng, &config);
            assert!(
                !has_frac_pi(&expr),
                "found FracPi with include_frac_pi=false at seed {}",
                seed
            );
        }
    }

    #[test]
    fn frac_pi_when_enabled() {
        let config = GenExprConfig {
            include_frac_pi: true,
            max_depth: 3,
            ..Default::default()
        };
        let found = (0..1000).any(|seed| {
            let mut rng = StdRng::seed_from_u64(seed);
            let expr = gen_expr(&mut rng, &config);
            has_frac_pi(&expr)
        });
        assert!(found, "no FracPi found in 1000 expressions with include_frac_pi=true");
    }

    #[test]
    fn roundtrip_through_tokenizer() {
        let config = GenExprConfig::default();
        for seed in 0..100 {
            let mut rng = StdRng::seed_from_u64(seed);
            let expr = gen_expr(&mut rng, &config);
            let (tokens, db) = tokenize(&expr);
            let result = detokenize(&tokens, &db);
            assert!(
                result.is_ok(),
                "tokenize/detokenize failed for seed {}: {:?}",
                seed,
                result.err()
            );
            assert_eq!(
                result.unwrap(),
                expr,
                "roundtrip mismatch for seed {}",
                seed
            );
        }
    }
}
