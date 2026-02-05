use crate::expr::Expr;
use crate::rule::RuleSet;

pub trait SearchStrategy {
    fn simplify(&self, expr: &Expr, rules: &RuleSet) -> Expr;
}

pub struct GreedySearch {
    pub max_steps: usize,
}

impl Default for GreedySearch {
    fn default() -> Self {
        GreedySearch { max_steps: 100 }
    }
}

impl GreedySearch {
    pub fn new(max_steps: usize) -> Self {
        GreedySearch { max_steps }
    }

    /// Apply simplification recursively in a single pass (bottom-up).
    /// Returns (simplified_expr, did_change).
    fn simplify_once(&self, expr: &Expr, rules: &RuleSet) -> (Expr, bool) {
        // First try to apply a rule at this node (before simplifying children)
        // This allows patterns like sin²x + cos²x to match before children are modified
        if let Some(result) = self.try_apply_rules_reducing(expr, rules) {
            return (result, true);
        }

        // Then simplify children
        let (with_simplified_children, children_changed) = self.simplify_children_once(expr, rules);

        // Try to apply a rule at this node again (after children simplified)
        if let Some(result) = self.try_apply_rules_reducing(&with_simplified_children, rules) {
            (result, true)
        } else {
            (with_simplified_children, children_changed)
        }
    }

    /// Simplify children of an expression in a single pass.
    /// Returns (expr_with_simplified_children, did_any_child_change).
    fn simplify_children_once(&self, expr: &Expr, rules: &RuleSet) -> (Expr, bool) {
        match expr {
            Expr::Const(_) | Expr::Var { .. } => (expr.clone(), false),

            Expr::Add(a, b) => {
                let (sa, ca) = self.simplify_once(a, rules);
                let (sb, cb) = self.simplify_once(b, rules);
                (Expr::Add(Box::new(sa), Box::new(sb)), ca || cb)
            }

            Expr::Mul(a, b) => {
                let (sa, ca) = self.simplify_once(a, rules);
                let (sb, cb) = self.simplify_once(b, rules);
                (Expr::Mul(Box::new(sa), Box::new(sb)), ca || cb)
            }

            Expr::Pow(base, exp) => {
                let (sb, cb) = self.simplify_once(base, rules);
                let (se, ce) = self.simplify_once(exp, rules);
                (Expr::Pow(Box::new(sb), Box::new(se)), cb || ce)
            }

            Expr::Neg(a) => {
                let (sa, ca) = self.simplify_once(a, rules);
                (Expr::Neg(Box::new(sa)), ca)
            }

            Expr::Inv(a) => {
                let (sa, ca) = self.simplify_once(a, rules);
                (Expr::Inv(Box::new(sa)), ca)
            }

            Expr::Fn(kind, a) => {
                let (sa, ca) = self.simplify_once(a, rules);
                (Expr::Fn(kind.clone(), Box::new(sa)), ca)
            }
        }
    }

    /// Try to apply any rule that reduces or maintains complexity.
    fn try_apply_rules_reducing(&self, expr: &Expr, rules: &RuleSet) -> Option<Expr> {
        let current_complexity = expr.complexity();

        for rule in rules.iter() {
            if let Some(result) = rule.apply_ltr(expr) {
                // Only accept the rule if it doesn't increase complexity
                if result.complexity() <= current_complexity {
                    return Some(result);
                }
            }
        }
        None
    }
}

impl SearchStrategy for GreedySearch {
    fn simplify(&self, expr: &Expr, rules: &RuleSet) -> Expr {
        let mut current = expr.clone();

        // Iterate until no more changes or max steps reached
        for _ in 0..self.max_steps {
            let (simplified, changed) = self.simplify_once(&current, rules);
            if !changed {
                return simplified;
            }
            current = simplified;
        }

        current
    }
}

/// Convenience function to simplify an expression with default settings.
pub fn simplify(expr: &Expr, rules: &RuleSet) -> Expr {
    GreedySearch::default().simplify(expr, rules)
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
}
