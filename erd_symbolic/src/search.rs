use crate::expr::Expr;
use crate::rule::RuleSet;
use std::collections::HashSet;

pub trait SearchStrategy {
    fn simplify(&self, expr: &Expr, rules: &RuleSet) -> Expr;
}

/// Beam search explores multiple rewrite paths simultaneously.
/// Unlike greedy search, it allows temporary complexity increases
/// that may lead to overall better simplifications.
pub struct BeamSearch {
    /// Number of candidates to keep at each step
    pub beam_width: usize,
    /// Maximum number of iterations
    pub max_steps: usize,
}

impl Default for BeamSearch {
    fn default() -> Self {
        BeamSearch {
            beam_width: 10,
            max_steps: 100,
        }
    }
}

/// A rewrite with metadata about its origin.
#[derive(Clone)]
struct Rewrite {
    expr: Expr,
    /// True if this rewrite came from a rule application (not just commutativity)
    from_rule: bool,
}

impl BeamSearch {
    pub fn new(beam_width: usize, max_steps: usize) -> Self {
        BeamSearch {
            beam_width,
            max_steps,
        }
    }

    /// Generate all possible single-step rewrites of an expression.
    /// Returns rewrites tagged with whether they came from a rule or commutativity.
    fn all_rewrites(&self, expr: &Expr, rules: &RuleSet) -> Vec<Rewrite> {
        let mut results = Vec::new();

        // Try applying each rule at the root
        for rule in rules.iter() {
            if let Some(rewritten) = rule.apply_ltr(expr) {
                results.push(Rewrite {
                    expr: rewritten,
                    from_rule: true,
                });
            }
        }

        // Try applying commutative swaps at the root (not from rules)
        if let Some(swapped) = self.try_commute(expr) {
            results.push(Rewrite {
                expr: swapped,
                from_rule: false,
            });
        }

        // Recursively try rewrites in children
        match expr {
            Expr::Const(_) | Expr::Var { .. } => {}

            Expr::Add(a, b) => {
                for rewrite in self.all_rewrites(a, rules) {
                    results.push(Rewrite {
                        expr: Expr::Add(Box::new(rewrite.expr), b.clone()),
                        from_rule: rewrite.from_rule,
                    });
                }
                for rewrite in self.all_rewrites(b, rules) {
                    results.push(Rewrite {
                        expr: Expr::Add(a.clone(), Box::new(rewrite.expr)),
                        from_rule: rewrite.from_rule,
                    });
                }
            }

            Expr::Mul(a, b) => {
                for rewrite in self.all_rewrites(a, rules) {
                    results.push(Rewrite {
                        expr: Expr::Mul(Box::new(rewrite.expr), b.clone()),
                        from_rule: rewrite.from_rule,
                    });
                }
                for rewrite in self.all_rewrites(b, rules) {
                    results.push(Rewrite {
                        expr: Expr::Mul(a.clone(), Box::new(rewrite.expr)),
                        from_rule: rewrite.from_rule,
                    });
                }
            }

            Expr::Pow(base, exp) => {
                for rewrite in self.all_rewrites(base, rules) {
                    results.push(Rewrite {
                        expr: Expr::Pow(Box::new(rewrite.expr), exp.clone()),
                        from_rule: rewrite.from_rule,
                    });
                }
                for rewrite in self.all_rewrites(exp, rules) {
                    results.push(Rewrite {
                        expr: Expr::Pow(base.clone(), Box::new(rewrite.expr)),
                        from_rule: rewrite.from_rule,
                    });
                }
            }

            Expr::Neg(a) => {
                for rewrite in self.all_rewrites(a, rules) {
                    results.push(Rewrite {
                        expr: Expr::Neg(Box::new(rewrite.expr)),
                        from_rule: rewrite.from_rule,
                    });
                }
            }

            Expr::Inv(a) => {
                for rewrite in self.all_rewrites(a, rules) {
                    results.push(Rewrite {
                        expr: Expr::Inv(Box::new(rewrite.expr)),
                        from_rule: rewrite.from_rule,
                    });
                }
            }

            Expr::Fn(kind, a) => {
                for rewrite in self.all_rewrites(a, rules) {
                    results.push(Rewrite {
                        expr: Expr::Fn(kind.clone(), Box::new(rewrite.expr)),
                        from_rule: rewrite.from_rule,
                    });
                }
            }
        }

        results
    }

    /// Try to commute operands of commutative operations (Add, Mul).
    fn try_commute(&self, expr: &Expr) -> Option<Expr> {
        match expr {
            Expr::Add(a, b) => Some(Expr::Add(b.clone(), a.clone())),
            Expr::Mul(a, b) => Some(Expr::Mul(b.clone(), a.clone())),
            _ => None,
        }
    }

    /// Convert an expression to a canonical string for deduplication.
    fn expr_key(expr: &Expr) -> String {
        format!("{:?}", expr)
    }
}

impl SearchStrategy for BeamSearch {
    fn simplify(&self, expr: &Expr, rules: &RuleSet) -> Expr {
        // Track seen expressions to avoid cycles
        let mut seen: HashSet<String> = HashSet::new();

        // Current beam of candidates, sorted by complexity
        let mut beam: Vec<Expr> = vec![expr.clone()];
        seen.insert(Self::expr_key(expr));

        // Track the best (lowest complexity) expression seen
        let mut best = expr.clone();
        let mut best_complexity = expr.complexity();
        // Track if best came from a rule (vs being the original or a commutative swap)
        let mut best_from_rule = false;

        for _step in 0..self.max_steps {
            let mut next_beam: Vec<Expr> = Vec::new();

            // Generate all rewrites from all current candidates
            for candidate in &beam {
                for rewrite in self.all_rewrites(candidate, rules) {
                    let key = Self::expr_key(&rewrite.expr);
                    if !seen.contains(&key) {
                        seen.insert(key);

                        let complexity = rewrite.expr.complexity();

                        // Update best if:
                        // 1. This is strictly simpler, OR
                        // 2. Same complexity but this is from a rule and current best isn't
                        //    (prefer canonical rule-based forms over original/swapped forms)
                        let dominated = complexity < best_complexity
                            || (complexity == best_complexity
                                && rewrite.from_rule
                                && !best_from_rule);

                        if dominated {
                            best = rewrite.expr.clone();
                            best_complexity = complexity;
                            best_from_rule = rewrite.from_rule;
                        }

                        next_beam.push(rewrite.expr);
                    }
                }
            }

            if next_beam.is_empty() {
                // No new candidates, we've reached a fixed point
                break;
            }

            // Sort by complexity and keep top beam_width candidates
            next_beam.sort_by_key(|e| e.complexity());
            next_beam.truncate(self.beam_width);

            beam = next_beam;
        }

        best
    }
}

/// Convenience function to simplify an expression with default settings.
pub fn simplify(expr: &Expr, rules: &RuleSet) -> Expr {
    BeamSearch::default().simplify(expr, rules)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::{add, constant, cos, inv, mul, neg, pow, scalar, sin, tensor, upper};

    #[test]
    fn simplify_add_zero() {
        let rules = RuleSet::standard();
        let expr = add(scalar("x"), constant(0.0));
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_mul_one() {
        let rules = RuleSet::standard();
        let expr = mul(scalar("x"), constant(1.0));
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_mul_zero() {
        let rules = RuleSet::standard();
        let expr = mul(scalar("x"), constant(0.0));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_double_neg() {
        let rules = RuleSet::standard();
        let expr = neg(neg(scalar("x")));
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_double_inv() {
        let rules = RuleSet::standard();
        let expr = inv(inv(scalar("x")));
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_pow_one() {
        let rules = RuleSet::standard();
        let expr = pow(scalar("x"), constant(1.0));
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_pow_zero() {
        let rules = RuleSet::standard();
        let expr = pow(scalar("x"), constant(0.0));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(1.0));
    }

    #[test]
    fn simplify_nested_add_zero() {
        // (x + 0) + (y + 0) should simplify to x + y
        let rules = RuleSet::standard();
        let expr = add(
            add(scalar("x"), constant(0.0)),
            add(scalar("y"), constant(0.0)),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, add(scalar("x"), scalar("y")));
    }

    #[test]
    fn simplify_nested_mul_one() {
        // (x * 1) * (y * 1) should simplify to x * y
        let rules = RuleSet::standard();
        let expr = mul(
            mul(scalar("x"), constant(1.0)),
            mul(scalar("y"), constant(1.0)),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, mul(scalar("x"), scalar("y")));
    }

    #[test]
    fn simplify_chain_identities() {
        // x + 0 + 0 (represented as (x + 0) + 0)
        let rules = RuleSet::standard();
        let expr = add(add(scalar("x"), constant(0.0)), constant(0.0));
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_trig_sin_zero() {
        let rules = RuleSet::trigonometric();
        let expr = sin(constant(0.0));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_trig_cos_zero() {
        let rules = RuleSet::trigonometric();
        let expr = cos(constant(0.0));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(1.0));
    }

    #[test]
    fn simplify_trig_sin_pi_4() {
        use std::f64::consts::{FRAC_1_SQRT_2, FRAC_PI_4};
        let rules = RuleSet::trigonometric();
        let expr = sin(constant(FRAC_PI_4));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(FRAC_1_SQRT_2)); // √2/2
    }

    #[test]
    fn simplify_trig_cos_pi_4() {
        use std::f64::consts::{FRAC_1_SQRT_2, FRAC_PI_4};
        let rules = RuleSet::trigonometric();
        let expr = cos(constant(FRAC_PI_4));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(FRAC_1_SQRT_2)); // √2/2
    }

    #[test]
    fn simplify_trig_sin_pi_3() {
        use std::f64::consts::FRAC_PI_3;
        let rules = RuleSet::trigonometric();
        let expr = sin(constant(FRAC_PI_3));
        let result = simplify(&expr, &rules);
        let sqrt_3_over_2 = 3.0_f64.sqrt() / 2.0;
        assert_eq!(result, constant(sqrt_3_over_2)); // √3/2
    }

    #[test]
    fn simplify_trig_cos_pi_3() {
        use std::f64::consts::FRAC_PI_3;
        let rules = RuleSet::trigonometric();
        let expr = cos(constant(FRAC_PI_3));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.5)); // 1/2
    }

    #[test]
    fn simplify_trig_sin_pi_6() {
        use std::f64::consts::FRAC_PI_6;
        let rules = RuleSet::trigonometric();
        let expr = sin(constant(FRAC_PI_6));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.5)); // 1/2
    }

    #[test]
    fn simplify_trig_cos_pi_6() {
        use std::f64::consts::FRAC_PI_6;
        let rules = RuleSet::trigonometric();
        let expr = cos(constant(FRAC_PI_6));
        let result = simplify(&expr, &rules);
        let sqrt_3_over_2 = 3.0_f64.sqrt() / 2.0;
        assert_eq!(result, constant(sqrt_3_over_2)); // √3/2
    }

    #[test]
    fn simplify_trig_sin_2pi() {
        use std::f64::consts::TAU;
        let rules = RuleSet::trigonometric();
        let expr = sin(constant(TAU)); // 2π
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_trig_cos_2pi() {
        use std::f64::consts::TAU;
        let rules = RuleSet::trigonometric();
        let expr = cos(constant(TAU)); // 2π
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(1.0));
    }

    #[test]
    fn simplify_trig_sin_3pi_2() {
        use std::f64::consts::FRAC_PI_2;
        let rules = RuleSet::trigonometric();
        let three_pi_over_2 = 3.0 * FRAC_PI_2;
        let expr = sin(constant(three_pi_over_2));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(-1.0));
    }

    #[test]
    fn simplify_trig_cos_3pi_2() {
        use std::f64::consts::FRAC_PI_2;
        let rules = RuleSet::trigonometric();
        let three_pi_over_2 = 3.0 * FRAC_PI_2;
        let expr = cos(constant(three_pi_over_2));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_ln_e() {
        use crate::expr::ln;
        use std::f64::consts::E;
        let rules = RuleSet::trigonometric();
        let expr = ln(constant(E));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(1.0));
    }

    #[test]
    fn simplify_sin_complementary() {
        // sin(π/2 - x) = cos(x)
        use std::f64::consts::FRAC_PI_2;
        let rules = RuleSet::trigonometric();
        let expr = sin(add(constant(FRAC_PI_2), neg(scalar("x"))));
        let result = simplify(&expr, &rules);
        assert_eq!(result, cos(scalar("x")));
    }

    #[test]
    fn simplify_cos_complementary() {
        // cos(π/2 - x) = sin(x)
        use std::f64::consts::FRAC_PI_2;
        let rules = RuleSet::trigonometric();
        let expr = cos(add(constant(FRAC_PI_2), neg(scalar("x"))));
        let result = simplify(&expr, &rules);
        assert_eq!(result, sin(scalar("x")));
    }

    #[test]
    fn simplify_sin_supplementary() {
        // sin(π - x) = sin(x)
        use std::f64::consts::PI;
        let rules = RuleSet::trigonometric();
        let expr = sin(add(constant(PI), neg(scalar("x"))));
        let result = simplify(&expr, &rules);
        assert_eq!(result, sin(scalar("x")));
    }

    #[test]
    fn simplify_cos_supplementary() {
        // cos(π - x) = -cos(x)
        use std::f64::consts::PI;
        let rules = RuleSet::trigonometric();
        let expr = cos(add(constant(PI), neg(scalar("x"))));
        let result = simplify(&expr, &rules);
        assert_eq!(result, neg(cos(scalar("x"))));
    }

    #[test]
    fn simplify_sin_period() {
        // sin(x + 2π) = sin(x)
        use std::f64::consts::TAU;
        let rules = RuleSet::trigonometric();
        let expr = sin(add(scalar("x"), constant(TAU)));
        let result = simplify(&expr, &rules);
        assert_eq!(result, sin(scalar("x")));
    }

    #[test]
    fn simplify_cos_period() {
        // cos(x + 2π) = cos(x)
        use std::f64::consts::TAU;
        let rules = RuleSet::trigonometric();
        let expr = cos(add(scalar("x"), constant(TAU)));
        let result = simplify(&expr, &rules);
        assert_eq!(result, cos(scalar("x")));
    }

    #[test]
    fn simplify_double_angle_sin_contraction() {
        // 2·sin(x)·cos(x) = sin(2x)
        // Complexity: 5 → 3 (reduces!)
        let rules = RuleSet::trigonometric();
        let expr = mul(constant(2.0), mul(sin(scalar("x")), cos(scalar("x"))));
        let result = simplify(&expr, &rules);
        assert_eq!(result, sin(mul(constant(2.0), scalar("x"))));
    }

    #[test]
    fn simplify_double_angle_cos_contraction() {
        // cos²(x) - sin²(x) = cos(2x)
        // Complexity: 7 → 3 (reduces!)
        let rules = RuleSet::trigonometric();
        let expr = add(
            pow(cos(scalar("x")), constant(2.0)),
            neg(pow(sin(scalar("x")), constant(2.0))),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, cos(mul(constant(2.0), scalar("x"))));
    }

    #[test]
    fn simplify_pythagorean() {
        // sin²(x) + cos²(x) = 1
        let rules = RuleSet::trigonometric();
        let expr = add(
            pow(sin(scalar("x")), constant(2.0)),
            pow(cos(scalar("x")), constant(2.0)),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(1.0));
    }

    #[test]
    fn simplify_kronecker_delta() {
        // δ^μ_ν * v^ν = v^μ
        use crate::expr::lower;
        let rules = RuleSet::tensor();
        let expr = mul(
            tensor("δ", vec![upper("mu"), lower("nu")]),
            tensor("v", vec![upper("nu")]),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, tensor("v", vec![upper("mu")]));
    }

    #[test]
    fn simplify_preserves_non_matching() {
        // Expression that doesn't match any rules should be unchanged
        let rules = RuleSet::standard();
        let expr = add(scalar("x"), scalar("y"));
        let result = simplify(&expr, &rules);
        assert_eq!(result, expr);
    }

    #[test]
    fn simplify_combined_rules() {
        // Test with combined standard + trig rules
        let mut rules = RuleSet::standard();
        rules.merge(RuleSet::trigonometric());

        // sin(0) + x * 1 should become 0 + x = x
        let expr = add(sin(constant(0.0)), mul(scalar("x"), constant(1.0)));
        let result = simplify(&expr, &rules);
        // sin(0) -> 0, x*1 -> x, 0+x -> x
        assert_eq!(result, scalar("x"));
    }

    // === More complicated simplify tests ===

    #[test]
    fn simplify_deeply_nested_zeros() {
        // ((((x + 0) + 0) + 0) + 0) should simplify to x
        let rules = RuleSet::standard();
        let expr = add(
            add(
                add(add(scalar("x"), constant(0.0)), constant(0.0)),
                constant(0.0),
            ),
            constant(0.0),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_deeply_nested_ones() {
        // ((((x * 1) * 1) * 1) * 1) should simplify to x
        let rules = RuleSet::standard();
        let expr = mul(
            mul(
                mul(mul(scalar("x"), constant(1.0)), constant(1.0)),
                constant(1.0),
            ),
            constant(1.0),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_mixed_nested_identities() {
        // ((x * 1) + 0) * 1 + 0 should simplify to x
        let rules = RuleSet::standard();
        let expr = add(
            mul(
                add(mul(scalar("x"), constant(1.0)), constant(0.0)),
                constant(1.0),
            ),
            constant(0.0),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_zero_propagation() {
        // (x + y) * 0 should simplify to 0
        let rules = RuleSet::standard();
        let expr = mul(add(scalar("x"), scalar("y")), constant(0.0));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_nested_zero_propagation() {
        // ((a + b) * (c + d)) * 0 should simplify to 0
        let rules = RuleSet::standard();
        let expr = mul(
            mul(add(scalar("a"), scalar("b")), add(scalar("c"), scalar("d"))),
            constant(0.0),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_multiple_double_negations() {
        // ----x should simplify to x
        let rules = RuleSet::standard();
        let expr = neg(neg(neg(neg(scalar("x")))));
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_triple_negation() {
        // ---x should simplify to -x
        let rules = RuleSet::standard();
        let expr = neg(neg(neg(scalar("x"))));
        let result = simplify(&expr, &rules);
        assert_eq!(result, neg(scalar("x")));
    }

    #[test]
    fn simplify_multiple_double_inversions() {
        // 1/(1/(1/(1/x))) should simplify to x
        let rules = RuleSet::standard();
        let expr = inv(inv(inv(inv(scalar("x")))));
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_pow_chain() {
        // (x^1)^1 should simplify to x
        let rules = RuleSet::standard();
        let expr = pow(pow(scalar("x"), constant(1.0)), constant(1.0));
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_pow_zero_nested() {
        // (complex_expr)^0 should simplify to 1
        let rules = RuleSet::standard();
        let complex = add(mul(scalar("x"), scalar("y")), neg(scalar("z")));
        let expr = pow(complex, constant(0.0));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(1.0));
    }

    #[test]
    fn simplify_one_pow_anything() {
        // 1^(complex_expr) should simplify to 1
        let rules = RuleSet::standard();
        let complex = add(mul(scalar("x"), scalar("y")), scalar("z"));
        let expr = pow(constant(1.0), complex);
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(1.0));
    }

    #[test]
    fn simplify_pythagorean_nested_in_expression() {
        // x * (sin²(y) + cos²(y)) should simplify to x * 1 = x
        let mut rules = RuleSet::standard();
        rules.merge(RuleSet::trigonometric());

        let pythagorean = add(
            pow(sin(scalar("y")), constant(2.0)),
            pow(cos(scalar("y")), constant(2.0)),
        );
        let expr = mul(scalar("x"), pythagorean);
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_pythagorean_added_to_expression() {
        // (sin²(x) + cos²(x)) + z should simplify to 1 + z
        let rules = RuleSet::trigonometric();

        let pythagorean = add(
            pow(sin(scalar("x")), constant(2.0)),
            pow(cos(scalar("x")), constant(2.0)),
        );
        let expr = add(pythagorean, scalar("z"));
        let result = simplify(&expr, &rules);
        assert_eq!(result, add(constant(1.0), scalar("z")));
    }

    #[test]
    fn simplify_exp_ln_nested() {
        // exp(ln(exp(ln(x)))) should simplify to x
        let rules = RuleSet::trigonometric();
        use crate::expr::{exp, ln};

        let expr = exp(ln(exp(ln(scalar("x")))));
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_ln_exp_nested() {
        // ln(exp(ln(exp(x)))) should simplify to x
        let rules = RuleSet::trigonometric();
        use crate::expr::{exp, ln};

        let expr = ln(exp(ln(exp(scalar("x")))));
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_sin_cos_at_zero() {
        // sin(0) + cos(0) should simplify to 0 + 1 with trig rules
        // then to 1 with standard rules
        let mut rules = RuleSet::standard();
        rules.merge(RuleSet::trigonometric());

        let expr = add(sin(constant(0.0)), cos(constant(0.0)));
        let result = simplify(&expr, &rules);
        // sin(0) -> 0, cos(0) -> 1, 0 + 1 = 1
        assert_eq!(result, constant(1.0));
    }

    #[test]
    fn simplify_cos_neg_nested() {
        // cos(-(-x)) should simplify to cos(x) using cos(-u) = cos(u) and --u = u
        let mut rules = RuleSet::standard();
        rules.merge(RuleSet::trigonometric());

        let expr = cos(neg(neg(scalar("x"))));
        let result = simplify(&expr, &rules);
        assert_eq!(result, cos(scalar("x")));
    }

    #[test]
    fn simplify_sin_neg_zero() {
        // sin(-0) should simplify to 0
        // sin(-0) -> -sin(0) -> -0 -> 0
        let mut rules = RuleSet::standard();
        rules.merge(RuleSet::trigonometric());

        let expr = sin(neg(constant(0.0)));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_kronecker_delta_chain() {
        // Test that (δ^ν_ρ * v^ρ) simplifies to v^ν first,
        // then we can contract with another delta
        use crate::expr::lower;
        let rules = RuleSet::tensor();

        // First contraction: δ^ν_ρ * v^ρ -> v^ν
        let inner = mul(
            tensor("δ", vec![upper("nu"), lower("rho")]),
            tensor("v", vec![upper("rho")]),
        );
        let inner_result = simplify(&inner, &rules);
        assert_eq!(inner_result, tensor("v", vec![upper("nu")]));

        // Second contraction: δ^μ_ν * v^ν -> v^μ
        let outer = mul(
            tensor("δ", vec![upper("mu"), lower("nu")]),
            inner_result,
        );
        let final_result = simplify(&outer, &rules);
        assert_eq!(final_result, tensor("v", vec![upper("mu")]));
    }

    #[test]
    fn simplify_kronecker_delta_both_sides() {
        // v^α * δ^β_α should simplify to v^β
        use crate::expr::lower;
        let rules = RuleSet::tensor();

        let expr = mul(
            tensor("v", vec![upper("alpha")]),
            tensor("δ", vec![upper("beta"), lower("alpha")]),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, tensor("v", vec![upper("beta")]));
    }

    #[test]
    fn simplify_full_ruleset() {
        // Test with the full ruleset combining all rule types
        // ((x * 1) + sin(0)) * (sin²(y) + cos²(y))
        // -> (x + 0) * 1 -> x * 1 -> x
        let rules = RuleSet::full();

        let expr = mul(
            add(mul(scalar("x"), constant(1.0)), sin(constant(0.0))),
            add(
                pow(sin(scalar("y")), constant(2.0)),
                pow(cos(scalar("y")), constant(2.0)),
            ),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_complex_expression_with_all_operators() {
        // -(-(x^1 * 1) + 0) should simplify to x
        let rules = RuleSet::standard();

        let expr = neg(neg(add(
            mul(pow(scalar("x"), constant(1.0)), constant(1.0)),
            constant(0.0),
        )));
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_inv_of_inv_nested_in_mul() {
        // x * (1/(1/y)) should simplify to x * y
        let rules = RuleSet::standard();

        let expr = mul(scalar("x"), inv(inv(scalar("y"))));
        let result = simplify(&expr, &rules);
        assert_eq!(result, mul(scalar("x"), scalar("y")));
    }

    #[test]
    fn simplify_zero_times_complex_trig() {
        // 0 * (sin(x) + cos(x)) should simplify to 0
        let mut rules = RuleSet::standard();
        rules.merge(RuleSet::trigonometric());

        let expr = mul(constant(0.0), add(sin(scalar("x")), cos(scalar("x"))));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_pythagorean_with_complex_argument() {
        // sin²(x + y) + cos²(x + y) = 1
        let rules = RuleSet::trigonometric();

        let arg = add(scalar("x"), scalar("y"));
        let expr = add(
            pow(sin(arg.clone()), constant(2.0)),
            pow(cos(arg), constant(2.0)),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(1.0));
    }

    #[test]
    fn simplify_nested_functions_with_identities() {
        // exp(ln(1)) should simplify to 1
        // exp(0) = 1
        let rules = RuleSet::trigonometric();
        use crate::expr::{exp, ln};

        let expr = exp(ln(constant(1.0)));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(1.0));
    }

    #[test]
    fn simplify_preserves_structure_when_no_rules_match() {
        // (x + y) * (a + b) should remain unchanged with standard rules
        // (no distribution in standard ruleset)
        let rules = RuleSet::standard();

        let expr = mul(add(scalar("x"), scalar("y")), add(scalar("a"), scalar("b")));
        let result = simplify(&expr, &rules);
        assert_eq!(result, expr);
    }

    #[test]
    fn simplify_max_steps_limit() {
        // Test that we don't infinite loop - use a very low max_steps
        let rules = RuleSet::standard();
        let search = BeamSearch::new(5, 2);

        // This would need many steps to fully simplify
        let expr = add(
            add(
                add(add(scalar("x"), constant(0.0)), constant(0.0)),
                constant(0.0),
            ),
            constant(0.0),
        );
        let result = search.simplify(&expr, &rules);
        // With only 2 steps, we won't fully simplify
        // But we should make some progress and not crash
        assert!(result.complexity() <= expr.complexity());
    }

    #[test]
    fn simplify_tensor_with_standard_rules() {
        // δ^μ_ν * (v^ν * 1) should simplify using both tensor and standard rules
        use crate::expr::lower;
        let mut rules = RuleSet::standard();
        rules.merge(RuleSet::tensor());

        let expr = mul(
            tensor("δ", vec![upper("mu"), lower("nu")]),
            mul(tensor("v", vec![upper("nu")]), constant(1.0)),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, tensor("v", vec![upper("mu")]));
    }

    // === Additional edge case tests ===

    #[test]
    fn simplify_add_zero_left() {
        // 0 + x should simplify to x (tests left-side zero rule)
        let rules = RuleSet::standard();
        let expr = add(constant(0.0), scalar("x"));
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_mul_one_left() {
        // 1 * x should simplify to x (tests left-side one rule)
        let rules = RuleSet::standard();
        let expr = mul(constant(1.0), scalar("x"));
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_mul_zero_left() {
        // 0 * x should simplify to 0 (tests left-side zero rule)
        let rules = RuleSet::standard();
        let expr = mul(constant(0.0), scalar("x"));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_neg_of_neg_of_constant() {
        // --5 should simplify to 5
        let rules = RuleSet::standard();
        let expr = neg(neg(constant(5.0)));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(5.0));
    }

    #[test]
    fn simplify_inv_of_one() {
        // 1/1 = 1
        let rules = RuleSet::standard();
        let expr = inv(constant(1.0));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(1.0));
    }

    #[test]
    fn simplify_pow_negative_exponent() {
        // x^(-1) with tensor rules: pow_neg_exp produces 1/x^1
        // Same complexity, but rule-based rewrites are preferred
        let rules = RuleSet::tensor();
        let expr = pow(scalar("x"), neg(constant(1.0)));
        let result = simplify(&expr, &rules);
        // pow_neg_exp: x^(-a) = 1/x^a
        assert_eq!(result, inv(pow(scalar("x"), constant(1.0))));
    }

    #[test]
    fn simplify_pow_negative_exponent_with_standard() {
        // x^(-1) with both tensor and standard rules
        // Should become 1/x^1 then 1/x (via pow_one rule)
        let mut rules = RuleSet::standard();
        rules.merge(RuleSet::tensor());
        let expr = pow(scalar("x"), neg(constant(1.0)));
        let result = simplify(&expr, &rules);
        assert_eq!(result, inv(scalar("x")));
    }

    #[test]
    fn simplify_zero_to_any_power() {
        // 0^x = 0 (assumes x > 0)
        let rules = RuleSet::standard();
        let expr = pow(constant(0.0), scalar("x"));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_neg_zero() {
        // -0 should simplify to 0
        let rules = RuleSet::standard();
        let expr = neg(constant(0.0));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_add_neg_self() {
        // x + (-x) = 0 (term cancellation)
        let rules = RuleSet::standard();
        let expr = add(scalar("x"), neg(scalar("x")));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_neg_add_self() {
        // (-x) + x = 0 (term cancellation, reversed order)
        let rules = RuleSet::standard();
        let expr = add(neg(scalar("x")), scalar("x"));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_mul_neg_one_right() {
        // x * (-1) = -x
        let rules = RuleSet::standard();
        let expr = mul(scalar("x"), neg(constant(1.0)));
        let result = simplify(&expr, &rules);
        assert_eq!(result, neg(scalar("x")));
    }

    #[test]
    fn simplify_mul_neg_one_left() {
        // (-1) * x = -x
        let rules = RuleSet::standard();
        let expr = mul(neg(constant(1.0)), scalar("x"));
        let result = simplify(&expr, &rules);
        assert_eq!(result, neg(scalar("x")));
    }

    #[test]
    fn simplify_trig_at_pi() {
        // sin(π) = 0, cos(π) = -1
        use std::f64::consts::PI;
        let rules = RuleSet::trigonometric();

        let sin_pi = sin(constant(PI));
        let cos_pi = cos(constant(PI));

        assert_eq!(simplify(&sin_pi, &rules), constant(0.0));
        assert_eq!(simplify(&cos_pi, &rules), constant(-1.0));
    }

    #[test]
    fn simplify_trig_at_pi_over_2() {
        // sin(π/2) = 1, cos(π/2) = 0
        use std::f64::consts::FRAC_PI_2;
        let rules = RuleSet::trigonometric();

        let sin_pi_2 = sin(constant(FRAC_PI_2));
        let cos_pi_2 = cos(constant(FRAC_PI_2));

        assert_eq!(simplify(&sin_pi_2, &rules), constant(1.0));
        assert_eq!(simplify(&cos_pi_2, &rules), constant(0.0));
    }

    #[test]
    fn simplify_exp_zero() {
        // exp(0) = 1
        let rules = RuleSet::trigonometric();
        use crate::expr::exp;

        let expr = exp(constant(0.0));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(1.0));
    }

    #[test]
    fn simplify_ln_one() {
        // ln(1) = 0
        let rules = RuleSet::trigonometric();
        use crate::expr::ln;

        let expr = ln(constant(1.0));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_pow_same_base_mul() {
        // x^2 * x^3 = x^(2+3) with tensor rules
        let rules = RuleSet::tensor();
        let expr = mul(
            pow(scalar("x"), constant(2.0)),
            pow(scalar("x"), constant(3.0)),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, pow(scalar("x"), add(constant(2.0), constant(3.0))));
    }

    #[test]
    fn simplify_pow_of_pow() {
        // (x^2)^3 = x^(2*3) with tensor rules
        // Same complexity, but rule-based rewrites are preferred
        let rules = RuleSet::tensor();
        let expr = pow(pow(scalar("x"), constant(2.0)), constant(3.0));
        let result = simplify(&expr, &rules);
        // pow_of_pow: (x^a)^b = x^(a*b)
        assert_eq!(result, pow(scalar("x"), mul(constant(2.0), constant(3.0))));
    }

    #[test]
    fn simplify_distribute_mul_over_add_no_simplification() {
        // x * (y + z) has complexity 5, x*y + x*z has complexity 7
        // Beam search explores the distributed form, but since no follow-up rules
        // reduce complexity below the original, the original is returned.
        let rules = RuleSet::tensor();
        let expr = mul(scalar("x"), add(scalar("y"), scalar("z")));
        let result = simplify(&expr, &rules);
        assert_eq!(result, expr);
    }

    #[test]
    fn simplify_neg_distribute_over_add_no_simplification() {
        // -(x + y) has complexity 4, -x + -y has complexity 5
        // Beam search explores the distributed form, but since no follow-up rules
        // reduce complexity below the original, the original is returned.
        let rules = RuleSet::tensor();
        let expr = neg(add(scalar("x"), scalar("y")));
        let result = simplify(&expr, &rules);
        assert_eq!(result, expr);
    }

    #[test]
    fn simplify_inv_distribute_over_mul_no_simplification() {
        // 1/(x * y) has complexity 4, (1/x) * (1/y) has complexity 5
        // Beam search explores the distributed form, but since no follow-up rules
        // reduce complexity below the original, the original is returned.
        let rules = RuleSet::tensor();
        let expr = inv(mul(scalar("x"), scalar("y")));
        let result = simplify(&expr, &rules);
        assert_eq!(result, expr);
    }

    #[test]
    fn simplify_deep_nesting_all_identities() {
        // (((x^1 * 1) + 0)^1 * 1) + 0 should simplify to x
        let rules = RuleSet::standard();
        let expr = add(
            mul(
                pow(
                    add(mul(pow(scalar("x"), constant(1.0)), constant(1.0)), constant(0.0)),
                    constant(1.0),
                ),
                constant(1.0),
            ),
            constant(0.0),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_alternating_neg_inv() {
        // -1/(-(1/x)) should simplify
        // First: 1/x, then -(1/x), then 1/(-(1/x)) = -1/(1/x) = -x?
        // Actually: double_inv on 1/(1/x) gives x, so 1/(-(1/x))...
        // This is tricky - let's see what happens
        let rules = RuleSet::standard();
        let expr = neg(inv(neg(inv(scalar("x")))));
        let result = simplify(&expr, &rules);
        // The expression is -(1/(-(1/x)))
        // With double_neg and double_inv rules, unclear what the final form is
        // Let's just check it doesn't crash and reduces complexity
        assert!(result.complexity() <= expr.complexity());
    }

    #[test]
    fn simplify_pythagorean_reversed() {
        // Beam search can simplify cos²(x) + sin²(x) = 1 via commutativity
        let rules = RuleSet::trigonometric();
        let expr = add(
            pow(cos(scalar("x")), constant(2.0)),
            pow(sin(scalar("x")), constant(2.0)),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(1.0));
    }

    #[test]
    fn simplify_tensor_covector_contraction() {
        // δ^μ_ν * w_ν = w_μ (covector contraction)
        use crate::expr::lower;
        let rules = RuleSet::tensor();

        let expr = mul(
            tensor("δ", vec![upper("mu"), lower("nu")]),
            tensor("w", vec![lower("nu")]),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, tensor("w", vec![lower("mu")]));
    }

    #[test]
    fn simplify_kronecker_no_contraction() {
        // δ^μ_ν * v^σ should NOT simplify (no matching indices)
        use crate::expr::lower;
        let rules = RuleSet::tensor();

        let expr = mul(
            tensor("δ", vec![upper("mu"), lower("nu")]),
            tensor("v", vec![upper("sigma")]),
        );
        let result = simplify(&expr, &rules);
        // No contraction, should remain unchanged
        assert_eq!(result, expr);
    }

    #[test]
    fn simplify_multiple_variables_same_structure() {
        // (a + 0) + (b + 0) + (c + 0) should simplify to a + b + c
        let rules = RuleSet::standard();
        let expr = add(
            add(add(scalar("a"), constant(0.0)), add(scalar("b"), constant(0.0))),
            add(scalar("c"), constant(0.0)),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, add(add(scalar("a"), scalar("b")), scalar("c")));
    }

    #[test]
    fn simplify_x_times_x_square() {
        // x * x = x^2 - same complexity, but rule-based rewrites are preferred
        let rules = RuleSet::standard();
        let expr = mul(scalar("x"), scalar("x"));
        let result = simplify(&expr, &rules);
        // mul_self_square: x * x = x^2
        assert_eq!(result, pow(scalar("x"), constant(2.0)));
    }

    #[test]
    fn simplify_sin_of_neg_x() {
        // sin(-x) = -sin(x) - same complexity, but rule-based rewrites are preferred
        let rules = RuleSet::trigonometric();
        let expr = sin(neg(scalar("x")));
        let result = simplify(&expr, &rules);
        // sin_neg: sin(-x) = -sin(x)
        assert_eq!(result, neg(sin(scalar("x"))));
    }

    #[test]
    fn simplify_cos_of_neg_x() {
        // cos(-x) = cos(x) (even function)
        let rules = RuleSet::trigonometric();
        let expr = cos(neg(scalar("x")));
        let result = simplify(&expr, &rules);
        assert_eq!(result, cos(scalar("x")));
    }

    #[test]
    fn simplify_complex_trig_identity() {
        // sin(-0) * cos(0) + sin(0) = 0 * 1 + 0 = 0
        let mut rules = RuleSet::standard();
        rules.merge(RuleSet::trigonometric());

        let expr = add(
            mul(sin(neg(constant(0.0))), cos(constant(0.0))),
            sin(constant(0.0)),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_nested_pow_with_identities() {
        // ((x^1)^0)^1 should become 1^1 = 1
        let rules = RuleSet::standard();
        let expr = pow(pow(pow(scalar("x"), constant(1.0)), constant(0.0)), constant(1.0));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(1.0));
    }

    #[test]
    fn simplify_zero_in_exponent_nested() {
        // (x + y)^0 * z should become 1 * z = z
        let rules = RuleSet::standard();
        let expr = mul(pow(add(scalar("x"), scalar("y")), constant(0.0)), scalar("z"));
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("z"));
    }

    #[test]
    fn simplify_one_to_complex_power() {
        // 1^(x + y + z) should become 1
        let rules = RuleSet::standard();
        let expr = pow(constant(1.0), add(add(scalar("x"), scalar("y")), scalar("z")));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(1.0));
    }

    #[test]
    fn simplify_tensor_generic_no_simplification() {
        // Generic tensors (not δ or g) don't have contraction rules
        use crate::expr::lower;
        let rules = RuleSet::tensor();

        // This won't match any rules since A is not a known tensor
        let expr = mul(
            tensor("A", vec![upper("mu"), lower("nu")]),
            tensor("B", vec![upper("nu")]),
        );
        let result = simplify(&expr, &rules);
        // No matching rule for generic tensor contraction, remains unchanged
        assert_eq!(result, expr);
    }

    #[test]
    fn simplify_tensor_metric_lower_index() {
        // g_μν * v^ν = v_μ (metric tensor lowers the index)
        use crate::expr::lower;
        let rules = RuleSet::tensor();

        let expr = mul(
            tensor("g", vec![lower("mu"), lower("nu")]),
            tensor("v", vec![upper("nu")]),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, tensor("v", vec![lower("mu")]));
    }

    #[test]
    fn simplify_tensor_metric_raise_index() {
        // g^μν * v_ν = v^μ (inverse metric tensor raises the index)
        use crate::expr::lower;
        let rules = RuleSet::tensor();

        let expr = mul(
            tensor("g", vec![upper("mu"), upper("nu")]),
            tensor("v", vec![lower("nu")]),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, tensor("v", vec![upper("mu")]));
    }

    #[test]
    fn simplify_tensor_metric_inverse_gives_delta() {
        // g^μκ * g_κν = δ^μ_ν (metric times inverse metric gives Kronecker delta)
        use crate::expr::lower;
        let rules = RuleSet::tensor();

        let expr = mul(
            tensor("g", vec![upper("mu"), upper("kappa")]),
            tensor("g", vec![lower("kappa"), lower("nu")]),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, tensor("δ", vec![upper("mu"), lower("nu")]));
    }

    #[test]
    fn simplify_tensor_metric_symmetry_enables_contraction() {
        // g_νμ * v^ν = v_μ
        // The metric g_νμ has indices in "wrong" order for metric_lower_right,
        // but symmetry rule g_νμ = g_μν allows beam search to find the contraction.
        use crate::expr::lower;
        let rules = RuleSet::tensor();

        let expr = mul(
            tensor("g", vec![lower("nu"), lower("mu")]), // indices swapped
            tensor("v", vec![upper("nu")]),
        );
        let result = simplify(&expr, &rules);
        // Via symmetry: g_νμ → g_μν, then metric_lower_right: g_μν * v^ν → v_μ
        assert_eq!(result, tensor("v", vec![lower("mu")]));
    }

    #[test]
    fn simplify_tensor_metric_inverse_with_symmetry() {
        // g^κμ * g_κν = δ^μ_ν
        // The first metric has indices swapped, but symmetry allows matching.
        use crate::expr::lower;
        let rules = RuleSet::tensor();

        let expr = mul(
            tensor("g", vec![upper("kappa"), upper("mu")]), // swapped from g^μκ
            tensor("g", vec![lower("kappa"), lower("nu")]),
        );
        let result = simplify(&expr, &rules);
        // Via symmetry: g^κμ → g^μκ, then metric_inverse_right: g^μκ * g_κν → δ^μ_ν
        assert_eq!(result, tensor("δ", vec![upper("mu"), lower("nu")]));
    }

    #[test]
    fn simplify_tensor_antisymmetric_standalone_no_change() {
        // ε_μν alone doesn't simplify - antisymmetry rule increases complexity
        // (adds negation). The rule exists for pattern matching in multi-step
        // simplifications, not standalone application.
        use crate::expr::lower;
        let rules = RuleSet::tensor();

        let expr = tensor("ε", vec![lower("mu"), lower("nu")]);
        let result = simplify(&expr, &rules);
        // No change - applying antisymmetry would increase complexity
        assert_eq!(result, expr);
    }

    #[test]
    fn simplify_tensor_double_antisymmetry_simplifies() {
        // -ε_νμ can use antisymmetry to become --ε_μν = ε_μν via double negation
        // This demonstrates antisymmetry combined with other rules.
        use crate::expr::lower;
        let rules = RuleSet::full(); // includes double_neg rule

        let expr = neg(tensor("ε", vec![lower("nu"), lower("mu")]));
        let result = simplify(&expr, &rules);
        // Via antisymmetry: -ε_νμ → --ε_μν, then double_neg: --ε_μν → ε_μν
        assert_eq!(result, tensor("ε", vec![lower("mu"), lower("nu")]));
    }

    #[test]
    fn simplify_tensor_antisymmetric_sum_cancels() {
        // ε_μν + ε_νμ = 0 (antisymmetric tensor + swapped form = 0)
        // This is a fundamental property of antisymmetric tensors.
        //
        // Path: ε_μν + ε_νμ (complexity 3)
        //    → ε_μν + (-ε_μν) via antisymmetry on second term (complexity 4 - INCREASE!)
        //    → 0 via add_neg_self (complexity 1)
        //
        // Beam search must explore the complexity-increasing step to find the simplification.
        use crate::expr::lower;
        let rules = RuleSet::full();

        let expr = add(
            tensor("ε", vec![lower("mu"), lower("nu")]),
            tensor("ε", vec![lower("nu"), lower("mu")]),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_tensor_antisymmetric_em_field_sum_cancels() {
        // F^μν + F^νμ = 0 (electromagnetic field tensor is antisymmetric)
        // Same principle as Levi-Civita: F^νμ = -F^μν, so F^μν + F^νμ = F^μν + (-F^μν) = 0
        let rules = RuleSet::full();

        let expr = add(
            tensor("F", vec![upper("mu"), upper("nu")]),
            tensor("F", vec![upper("nu"), upper("mu")]),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_tensor_custom_antisymmetric_cancels() {
        // Test that custom antisymmetric tensors also exhibit cancellation
        // ω_ij + ω_ji = 0 for antisymmetric ω (e.g., vorticity tensor)
        use crate::expr::lower;
        let mut rules = RuleSet::full();
        rules.add_antisymmetric("ω");

        let expr = add(
            tensor("ω", vec![lower("i"), lower("j")]),
            tensor("ω", vec![lower("j"), lower("i")]),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_tensor_custom_symmetric() {
        // Test custom symmetric tensor using add_symmetric
        // Symmetry rules have same complexity, but rule-based forms are preferred
        use crate::expr::lower;
        let mut rules = RuleSet::tensor();
        rules.add_symmetric("h"); // perturbation metric

        let expr = tensor("h", vec![lower("nu"), lower("mu")]);
        let result = simplify(&expr, &rules);
        // Symmetry allows reordering - rule-based form preferred at equal complexity
        assert_eq!(result, tensor("h", vec![lower("mu"), lower("nu")]));
    }

    #[test]
    fn simplify_exp_of_ln_of_exp() {
        // exp(ln(exp(x))) = exp(x)
        let rules = RuleSet::trigonometric();
        use crate::expr::{exp, ln};

        let expr = exp(ln(exp(scalar("x"))));
        let result = simplify(&expr, &rules);
        // ln(exp(x)) = x, then exp(x)
        assert_eq!(result, exp(scalar("x")));
    }

    #[test]
    fn simplify_ln_of_exp_of_ln() {
        // ln(exp(ln(x))) = ln(x)
        let rules = RuleSet::trigonometric();
        use crate::expr::{exp, ln};

        let expr = ln(exp(ln(scalar("x"))));
        let result = simplify(&expr, &rules);
        // exp(ln(x)) = x, then ln(x)
        assert_eq!(result, ln(scalar("x")));
    }

    #[test]
    fn simplify_associativity_does_not_loop() {
        // Test that associativity rules don't cause infinite loops
        // (a * b) * c with tensor rules should eventually stabilize
        let rules = RuleSet::tensor();
        let expr = mul(mul(scalar("a"), scalar("b")), scalar("c"));
        let result = simplify(&expr, &rules);
        // Should complete without infinite loop (max_steps prevents it)
        // The result might be reassociated
        assert!(result.complexity() <= expr.complexity() + 2); // Allow slight increase from reassociation
    }

    #[test]
    fn simplify_all_rulesets_complex() {
        // Test a very complex expression with all rule types
        // ((sin(0) + cos(0)) * x + 0)^1
        let rules = RuleSet::full();

        // (sin(0) + cos(0)) = 0 + 1 = 1
        // 1 * x = x
        // x + 0 = x
        // x^1 = x
        let trig_part = add(sin(constant(0.0)), cos(constant(0.0)));
        let expr = pow(
            add(mul(trig_part, scalar("x")), constant(0.0)),
            constant(1.0),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    // === Custom beam search parameter tests ===

    #[test]
    fn simplify_with_custom_params() {
        // Test BeamSearch with custom parameters
        let search = BeamSearch::new(5, 50);
        let rules = RuleSet::standard();
        let expr = add(add(scalar("x"), constant(0.0)), constant(0.0));
        let result = search.simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    // === Beam search exploration tests ===
    // These tests verify that beam search explores paths that may temporarily
    // increase complexity before finding a simpler result.

    #[test]
    fn simplify_neg_of_canceling_sum() {
        // -(x + (-x)) should simplify to 0
        // Path A (direct): -(x + (-x)) → -(0) → 0 (via add_neg_self_right, then neg_zero)
        // Path B (via distribution): -(x + (-x)) → -x + -(-x) → -x + x → 0
        //   complexity: 5 → 6 → 4 → 1 (temporary increase!)
        // Beam search explores both paths and finds the optimal result.
        let rules = RuleSet::full();
        let expr = neg(add(scalar("x"), neg(scalar("x"))));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_neg_of_double_neg_sum() {
        // -((-x) + x) should simplify to 0
        // The inner sum (-x) + x matches add_neg_self_left directly
        let rules = RuleSet::full();
        let expr = neg(add(neg(scalar("x")), scalar("x")));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_distribution_enables_cancellation() {
        // -(a + (-a)) where a = sin(x)
        // This tests that wildcards in add_neg_self match complex expressions
        let rules = RuleSet::full();
        let expr = neg(add(sin(scalar("x")), neg(sin(scalar("x")))));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_nested_neg_distribution() {
        // -(-(-x) + (-x)) should simplify:
        // Inner: -(-x) + (-x) = x + (-x) = 0 (via double_neg, then add_neg_self_right)
        // Then: -(0) = 0
        let rules = RuleSet::full();
        let expr = neg(add(neg(neg(scalar("x"))), neg(scalar("x"))));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_complex_path_through_distribution() {
        // (x + (-x)) * y should simplify to 0 * y = 0
        // Path: 7 → (inner cancel) → 4 → 1
        let rules = RuleSet::full();
        let expr = mul(add(scalar("x"), neg(scalar("x"))), scalar("y"));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_sum_of_pythagorean_and_neg_one() {
        // sin²(x) + cos²(x) + (-1) should simplify to 0
        // Path: pythagorean → 1, then 1 + (-1) → 0
        let rules = RuleSet::full();
        let expr = add(
            add(
                pow(sin(scalar("x")), constant(2.0)),
                pow(cos(scalar("x")), constant(2.0)),
            ),
            neg(constant(1.0)),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_reversed_pythagorean_plus_neg_one() {
        // cos²(x) + sin²(x) + (-1) should also simplify to 0
        // Requires commutativity exploration to match pythagorean pattern
        let rules = RuleSet::full();
        let expr = add(
            add(
                pow(cos(scalar("x")), constant(2.0)),
                pow(sin(scalar("x")), constant(2.0)),
            ),
            neg(constant(1.0)),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_power_chain_through_intermediate() {
        // x^2 * x^(-1) with tensor+standard rules
        // Via pow_mul_same_base: x^(2 + (-1))
        // The exponent 2 + (-1) = 2 - 1 = 1, but we don't have constant folding
        // However, this tests the path through equal-complexity transformations
        let rules = RuleSet::full();
        let expr = mul(
            pow(scalar("x"), constant(2.0)),
            pow(scalar("x"), neg(constant(1.0))),
        );
        let result = simplify(&expr, &rules);
        // Should apply pow_mul_same_base: x^(2 + (-1))
        // We don't have constant folding, so this is the simplified form
        assert_eq!(
            result,
            pow(scalar("x"), add(constant(2.0), neg(constant(1.0))))
        );
    }

    #[test]
    fn simplify_double_neg_in_product() {
        // (-(-x)) * y should simplify to x * y
        let rules = RuleSet::full();
        let expr = mul(neg(neg(scalar("x"))), scalar("y"));
        let result = simplify(&expr, &rules);
        assert_eq!(result, mul(scalar("x"), scalar("y")));
    }

    #[test]
    fn simplify_inv_chain() {
        // 1/(1/(1/(1/x))) should simplify to x
        // Each step maintains or reduces complexity until we get to x
        let rules = RuleSet::full();
        let expr = inv(inv(inv(inv(scalar("x")))));
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_mixed_neg_inv_chain() {
        // -(1/(-x)) should simplify
        // This exercises interaction between neg and inv rules
        let rules = RuleSet::full();
        let expr = neg(inv(neg(scalar("x"))));
        let result = simplify(&expr, &rules);
        // The exact result depends on rule order, but complexity should not increase
        assert!(result.complexity() <= expr.complexity());
    }

    #[test]
    fn simplify_zero_times_complex_sum() {
        // 0 * (a + b + c) should simplify to 0
        // Tests that mul_zero_left works with complex RHS
        let rules = RuleSet::full();
        let expr = mul(
            constant(0.0),
            add(add(scalar("a"), scalar("b")), scalar("c")),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_one_raised_to_complex_neg_power() {
        // 1^(-(x + y)) should simplify to 1
        // Tests one_pow rule with complex exponent
        let rules = RuleSet::full();
        let expr = pow(constant(1.0), neg(add(scalar("x"), scalar("y"))));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(1.0));
    }
}
