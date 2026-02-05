use crate::expr::{Expr, FnKind};
use std::collections::HashMap;

pub enum Pattern {
    Wildcard(String),
    ConstWild(String),
    Const(f64),
    Add(Box<Pattern>, Box<Pattern>),
    Mul(Box<Pattern>, Box<Pattern>),
    Neg(Box<Pattern>),
    Inv(Box<Pattern>),
    Pow(Box<Pattern>, Box<Pattern>),
    Fn(FnKind, Box<Pattern>),
}

pub struct Rule {
    pub name: String,
    pub lhs: Pattern,
    pub rhs: Pattern,
}

pub type Bindings = HashMap<String, Expr>;

impl Pattern {
    /// Attempt to match this pattern against an expression.
    /// Returns Some(bindings) if the match succeeds, None otherwise.
    /// Wildcards bind to the matched sub-expression by name.
    pub fn match_expr(&self, expr: &Expr) -> Option<Bindings> {
        let mut bindings = Bindings::new();
        if self.match_expr_inner(expr, &mut bindings) {
            Some(bindings)
        } else {
            None
        }
    }

    fn match_expr_inner(&self, expr: &Expr, bindings: &mut Bindings) -> bool {
        match (self, expr) {
            // Wildcard matches any expression
            (Pattern::Wildcard(name), _) => bind(name, expr.clone(), bindings),

            // ConstWild matches only constants
            (Pattern::ConstWild(name), Expr::Const(_)) => bind(name, expr.clone(), bindings),
            (Pattern::ConstWild(_), _) => false,

            // Const matches equal constants
            (Pattern::Const(n), Expr::Const(m)) => (n - m).abs() < f64::EPSILON,
            (Pattern::Const(_), _) => false,

            // Structural matching for binary operators
            (Pattern::Add(pa, pb), Expr::Add(a, b)) => {
                pa.match_expr_inner(a, bindings) && pb.match_expr_inner(b, bindings)
            }
            (Pattern::Add(_, _), _) => false,

            (Pattern::Mul(pa, pb), Expr::Mul(a, b)) => {
                pa.match_expr_inner(a, bindings) && pb.match_expr_inner(b, bindings)
            }
            (Pattern::Mul(_, _), _) => false,

            (Pattern::Pow(pb, pe), Expr::Pow(base, exp)) => {
                pb.match_expr_inner(base, bindings) && pe.match_expr_inner(exp, bindings)
            }
            (Pattern::Pow(_, _), _) => false,

            // Structural matching for unary operators
            (Pattern::Neg(p), Expr::Neg(a)) => p.match_expr_inner(a, bindings),
            (Pattern::Neg(_), _) => false,

            (Pattern::Inv(p), Expr::Inv(a)) => p.match_expr_inner(a, bindings),
            (Pattern::Inv(_), _) => false,

            // Function matching requires same function kind
            (Pattern::Fn(pk, p), Expr::Fn(ek, a)) if pk == ek => p.match_expr_inner(a, bindings),
            (Pattern::Fn(_, _), _) => false,
        }
    }

    pub fn substitute(&self, bindings: &Bindings) -> Expr {
        Expr::Const(0.0) // TODO: implement me
    }
}

/// Bind a name to an expression, checking consistency with existing bindings.
fn bind(name: &str, expr: Expr, bindings: &mut Bindings) -> bool {
    match bindings.get(name) {
        Some(existing) => *existing == expr, // Must match existing binding
        None => {
            bindings.insert(name.to_string(), expr);
            true
        }
    }
}

impl Rule {
    pub fn apply_ltr(&self, expr: &Expr) -> Option<Expr> {
        None // TODO: implement me
    }
    pub fn apply_rtl(&self, expr: &Expr) -> Option<Expr> {
        None // TODO: implement me
    }
}

pub struct RuleSet {
    rules: Vec<Rule>,
}

impl RuleSet {
    pub fn new() -> Self {
        RuleSet { rules: Vec::new() }
    }

    pub fn add(&mut self, rule: Rule) -> &mut Self {
        self // TODO: implement me
    }

    pub fn merge(&mut self, other: RuleSet) -> &mut Self {
        self // TODO: implement me
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::{add, constant, mul, neg, pow, scalar};

    // Pattern builder helpers
    fn wildcard(name: &str) -> Pattern {
        Pattern::Wildcard(name.to_string())
    }

    fn const_wild(name: &str) -> Pattern {
        Pattern::ConstWild(name.to_string())
    }

    fn p_add(a: Pattern, b: Pattern) -> Pattern {
        Pattern::Add(Box::new(a), Box::new(b))
    }

    fn p_mul(a: Pattern, b: Pattern) -> Pattern {
        Pattern::Mul(Box::new(a), Box::new(b))
    }

    fn p_neg(a: Pattern) -> Pattern {
        Pattern::Neg(Box::new(a))
    }

    #[test]
    fn match_wildcard() {
        let pattern = wildcard("x");
        let expr = scalar("a");
        let bindings = pattern.match_expr(&expr).unwrap();
        assert_eq!(bindings.get("x"), Some(&expr));
    }

    #[test]
    fn match_const_wild_succeeds() {
        let pattern = const_wild("c");
        let expr = constant(42.0);
        let bindings = pattern.match_expr(&expr).unwrap();
        assert_eq!(bindings.get("c"), Some(&expr));
    }

    #[test]
    fn match_const_wild_fails_on_non_const() {
        let pattern = const_wild("c");
        let expr = scalar("x");
        assert!(pattern.match_expr(&expr).is_none());
    }

    #[test]
    fn match_const_exact() {
        let pattern = Pattern::Const(3.0);
        assert!(pattern.match_expr(&constant(3.0)).is_some());
        assert!(pattern.match_expr(&constant(4.0)).is_none());
    }

    #[test]
    fn match_add_structure() {
        // Pattern: x + y
        let pattern = p_add(wildcard("x"), wildcard("y"));
        // Expression: a + b
        let expr = add(scalar("a"), scalar("b"));
        let bindings = pattern.match_expr(&expr).unwrap();
        assert_eq!(bindings.get("x"), Some(&scalar("a")));
        assert_eq!(bindings.get("y"), Some(&scalar("b")));
    }

    #[test]
    fn match_add_fails_on_mul() {
        let pattern = p_add(wildcard("x"), wildcard("y"));
        let expr = mul(scalar("a"), scalar("b"));
        assert!(pattern.match_expr(&expr).is_none());
    }

    #[test]
    fn match_nested_structure() {
        // Pattern: (x + y) * z
        let pattern = p_mul(p_add(wildcard("x"), wildcard("y")), wildcard("z"));
        // Expression: (a + b) * c
        let expr = mul(add(scalar("a"), scalar("b")), scalar("c"));
        let bindings = pattern.match_expr(&expr).unwrap();
        assert_eq!(bindings.get("x"), Some(&scalar("a")));
        assert_eq!(bindings.get("y"), Some(&scalar("b")));
        assert_eq!(bindings.get("z"), Some(&scalar("c")));
    }

    #[test]
    fn match_repeated_wildcard_same_expr() {
        // Pattern: x + x (same wildcard used twice)
        let pattern = p_add(wildcard("x"), wildcard("x"));
        // Expression: a + a (same sub-expression)
        let expr = add(scalar("a"), scalar("a"));
        let bindings = pattern.match_expr(&expr).unwrap();
        assert_eq!(bindings.get("x"), Some(&scalar("a")));
    }

    #[test]
    fn match_repeated_wildcard_different_expr_fails() {
        // Pattern: x + x (same wildcard used twice)
        let pattern = p_add(wildcard("x"), wildcard("x"));
        // Expression: a + b (different sub-expressions)
        let expr = add(scalar("a"), scalar("b"));
        assert!(pattern.match_expr(&expr).is_none());
    }

    #[test]
    fn match_neg() {
        let pattern = p_neg(wildcard("x"));
        let expr = neg(scalar("a"));
        let bindings = pattern.match_expr(&expr).unwrap();
        assert_eq!(bindings.get("x"), Some(&scalar("a")));
    }

    #[test]
    fn match_complex_algebraic_identity() {
        // Pattern for x^2: x * x
        let pattern = p_mul(wildcard("x"), wildcard("x"));
        // Expression: a * a
        let expr = mul(scalar("a"), scalar("a"));
        let bindings = pattern.match_expr(&expr).unwrap();
        assert_eq!(bindings.get("x"), Some(&scalar("a")));
    }
}
