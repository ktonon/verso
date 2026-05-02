//! Symbolic differentiation.
//!
//! Provides [`differentiate`], which takes an expression and a variable name
//! and returns the symbolic derivative. The function recursively walks the
//! expression and applies standard differentiation rules: power rule, sum,
//! product, quotient, chain rule, and the derivatives of the trig/exp/ln
//! functions registered in [`FnKind`].
//!
//! Discontinuous functions (`Sign`, `Floor`, `Ceil`, `Round`, `Min`, `Max`,
//! `Clamp`) are left as unevaluated `Diff` nodes — the symbolic form cannot
//! be reduced for those cases.

use crate::expr::{
    add, cos, cosh, diff, exp, inv, ln, mul, neg, pow, rational, sin, sinh, Expr, ExprKind, FnKind,
};

/// Returns the symbolic derivative of `expr` with respect to the variable
/// named `var`. The result has not been simplified — call `simplify` or run
/// it through the normal simplification pipeline to clean up trivial
/// `+ 0`, `* 1`, etc.
pub fn differentiate(expr: &Expr, var: &str) -> Expr {
    match &expr.kind {
        ExprKind::Rational(_) | ExprKind::FracPi(_) | ExprKind::Named(_) => zero(),

        ExprKind::Var { name, .. } => {
            if name == var {
                one()
            } else {
                zero()
            }
        }

        ExprKind::Add(a, b) => add(differentiate(a, var), differentiate(b, var)),

        ExprKind::Mul(a, b) => add(
            mul(differentiate(a, var), b.as_ref().clone()),
            mul(a.as_ref().clone(), differentiate(b, var)),
        ),

        ExprKind::Neg(inner) => neg(differentiate(inner, var)),

        ExprKind::Inv(inner) => {
            // d/dx (1/u) = -u' / u^2
            let du = differentiate(inner, var);
            let u_squared = pow(inner.as_ref().clone(), rational(2, 1));
            neg(mul(du, inv(u_squared)))
        }

        ExprKind::Pow(base, exp_node) => differentiate_pow(base, exp_node, var),

        ExprKind::Fn(kind, arg) => differentiate_fn(kind, arg, var),

        ExprKind::FnN(FnKind::Diff, _) => {
            // Nested Diff — leave as-is; the simplifier should have evaluated
            // the inner Diff first when running bottom-up.
            unevaluated(expr, var)
        }

        ExprKind::FnN(_, _) => {
            // Multi-arg functions (Min, Max, Clamp, custom) — leave unevaluated.
            // Custom functions should be expanded by Context before
            // differentiate is called; if we got here with a Custom, leave
            // the Diff node as-is.
            unevaluated(expr, var)
        }

        ExprKind::Quantity(inner, _) => {
            // Differentiate the inner expression. We drop the unit on the
            // result — keeping it would require dimensional analysis on
            // the variable, which is out of scope.
            differentiate(inner, var)
        }
    }
}

fn differentiate_pow(base: &Expr, exp_node: &Expr, var: &str) -> Expr {
    let base_depends = depends_on(base, var);
    let exp_depends = depends_on(exp_node, var);

    match (base_depends, exp_depends) {
        (false, false) => zero(),
        (true, false) => {
            // d/dx (u^n) = n * u^(n-1) * u'
            // where n is constant w.r.t. x.
            let exp_minus_one = add(exp_node.clone(), rational(-1, 1));
            let new_pow = pow(base.clone(), exp_minus_one);
            let du = differentiate(base, var);
            mul(mul(exp_node.clone(), new_pow), du)
        }
        (false, true) => {
            // d/dx (a^v) = a^v * ln(a) * v'
            // where a is constant w.r.t. x.
            let pow_self = pow(base.clone(), exp_node.clone());
            let dv = differentiate(exp_node, var);
            mul(mul(pow_self, ln(base.clone())), dv)
        }
        (true, true) => {
            // General logarithmic differentiation:
            // d/dx (u^v) = u^v * (v' * ln(u) + v * u' / u)
            let pow_self = pow(base.clone(), exp_node.clone());
            let du = differentiate(base, var);
            let dv = differentiate(exp_node, var);
            let term1 = mul(dv, ln(base.clone()));
            let term2 = mul(mul(exp_node.clone(), du), inv(base.clone()));
            mul(pow_self, add(term1, term2))
        }
    }
}

fn differentiate_fn(kind: &FnKind, arg: &Expr, var: &str) -> Expr {
    let du = differentiate(arg, var);
    match kind {
        // Trig
        FnKind::Sin => mul(cos(arg.clone()), du),
        FnKind::Cos => neg(mul(sin(arg.clone()), du)),
        FnKind::Tan => {
            // d/dx tan(u) = sec(u)^2 * u' = (1/cos(u))^2 * u'
            let sec_squared = pow(inv(cos(arg.clone())), rational(2, 1));
            mul(sec_squared, du)
        }
        FnKind::Asin => {
            // d/dx asin(u) = u' / sqrt(1 - u^2)
            let one_minus_u_sq = add(rational(1, 1), neg(pow(arg.clone(), rational(2, 1))));
            let sqrt = pow(one_minus_u_sq, rational(1, 2));
            mul(du, inv(sqrt))
        }
        FnKind::Acos => {
            // d/dx acos(u) = -u' / sqrt(1 - u^2)
            let one_minus_u_sq = add(rational(1, 1), neg(pow(arg.clone(), rational(2, 1))));
            let sqrt = pow(one_minus_u_sq, rational(1, 2));
            neg(mul(du, inv(sqrt)))
        }
        FnKind::Atan => {
            // d/dx atan(u) = u' / (1 + u^2)
            let one_plus_u_sq = add(rational(1, 1), pow(arg.clone(), rational(2, 1)));
            mul(du, inv(one_plus_u_sq))
        }

        // Hyperbolic
        FnKind::Sinh => mul(cosh(arg.clone()), du),
        FnKind::Cosh => mul(sinh(arg.clone()), du),
        FnKind::Tanh => {
            // d/dx tanh(u) = sech(u)^2 * u' = (1/cosh(u))^2 * u'
            let sech_squared = pow(inv(cosh(arg.clone())), rational(2, 1));
            mul(sech_squared, du)
        }

        // Exp / Ln
        FnKind::Exp => mul(exp(arg.clone()), du),
        FnKind::Ln => mul(du, inv(arg.clone())),

        // Discontinuous — leave the Diff node unevaluated.
        FnKind::Sign | FnKind::Floor | FnKind::Ceil | FnKind::Round => {
            let inner = Expr::new(ExprKind::Fn(kind.clone(), Box::new(arg.clone())));
            unevaluated(&inner, var)
        }

        FnKind::Min | FnKind::Max | FnKind::Clamp | FnKind::Diff => {
            // These are FnN; should not appear in the single-arg Fn match.
            // Defensive case: leave unevaluated.
            let inner = Expr::new(ExprKind::Fn(kind.clone(), Box::new(arg.clone())));
            unevaluated(&inner, var)
        }

        FnKind::Custom(_) => {
            // Custom functions should be expanded by the Context before
            // differentiation. If we got one here, leave unevaluated.
            let inner = Expr::new(ExprKind::Fn(kind.clone(), Box::new(arg.clone())));
            unevaluated(&inner, var)
        }
    }
}

fn depends_on(expr: &Expr, var: &str) -> bool {
    let mut found = false;
    expr.walk(&mut |e| {
        if let ExprKind::Var { name, .. } = &e.kind {
            if name == var {
                found = true;
            }
        }
    });
    found
}

fn zero() -> Expr {
    rational(0, 1)
}

fn one() -> Expr {
    rational(1, 1)
}

fn unevaluated(expr: &Expr, var: &str) -> Expr {
    diff(
        expr.clone(),
        Expr::new(ExprKind::Var {
            name: var.to_string(),
            indices: Vec::new(),
            dim: None,
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::simplify;
    use crate::{parse_expr, RuleSet};

    fn diff_simplified(expr: &str, var: &str) -> Expr {
        let parsed = parse_expr(expr).expect("parse");
        let result = differentiate(&parsed, var);
        simplify(&result, &RuleSet::full())
    }

    fn parse(s: &str) -> Expr {
        parse_expr(s).expect("parse")
    }

    fn assert_eq_simplified(actual: Expr, expected: &str) {
        let expected_expr = simplify(&parse(expected), &RuleSet::full());
        let actual_simplified = simplify(&actual, &RuleSet::full());
        assert_eq!(
            actual_simplified, expected_expr,
            "expected {} got {}",
            expected_expr, actual_simplified
        );
    }

    #[test]
    fn diff_constant_is_zero() {
        let result = diff_simplified("5", "x");
        assert_eq_simplified(result, "0");
    }

    #[test]
    fn diff_var_match_is_one() {
        let result = diff_simplified("x", "x");
        assert_eq_simplified(result, "1");
    }

    #[test]
    fn diff_other_var_is_zero() {
        let result = diff_simplified("y", "x");
        assert_eq_simplified(result, "0");
    }

    #[test]
    fn diff_power_rule() {
        let result = diff_simplified("x^3", "x");
        assert_eq_simplified(result, "3 * x^2");
    }

    #[test]
    fn diff_sum() {
        let result = diff_simplified("x + x^2", "x");
        assert_eq_simplified(result, "1 + 2*x");
    }

    #[test]
    fn diff_quotient_m_over_r() {
        // The motivating tmm case: d/dr (M/r) = -M/r^2
        let result = diff_simplified("M / r", "r");
        assert_eq_simplified(result, "-M / r^2");
    }

    #[test]
    fn diff_product_with_sin() {
        // d/dx (x * sin(x)) = sin(x) + x*cos(x)
        let result = diff_simplified("x * sin(x)", "x");
        assert_eq_simplified(result, "sin(x) + x * cos(x)");
    }

    #[test]
    fn diff_chain_rule_sin_x_squared() {
        // d/dx sin(x^2) = 2x * cos(x^2)
        let result = diff_simplified("sin(x^2)", "x");
        assert_eq_simplified(result, "2 * x * cos(x^2)");
    }

    #[test]
    fn diff_exp() {
        let result = diff_simplified("exp(x)", "x");
        assert_eq_simplified(result, "exp(x)");
    }

    #[test]
    fn diff_ln() {
        let result = diff_simplified("ln(x)", "x");
        assert_eq_simplified(result, "1/x");
    }

    #[test]
    fn diff_constant_times_var() {
        let result = diff_simplified("3 * x", "x");
        assert_eq_simplified(result, "3");
    }
}
