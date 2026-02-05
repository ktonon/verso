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

    /// Substitute bindings into this pattern to produce an expression.
    /// Panics if a wildcard is not found in bindings.
    pub fn substitute(&self, bindings: &Bindings) -> Expr {
        match self {
            Pattern::Wildcard(name) | Pattern::ConstWild(name) => bindings
                .get(name)
                .unwrap_or_else(|| panic!("Unbound wildcard: {}", name))
                .clone(),

            Pattern::Const(n) => Expr::Const(*n),

            Pattern::Add(pa, pb) => Expr::Add(
                Box::new(pa.substitute(bindings)),
                Box::new(pb.substitute(bindings)),
            ),

            Pattern::Mul(pa, pb) => Expr::Mul(
                Box::new(pa.substitute(bindings)),
                Box::new(pb.substitute(bindings)),
            ),

            Pattern::Pow(pb, pe) => Expr::Pow(
                Box::new(pb.substitute(bindings)),
                Box::new(pe.substitute(bindings)),
            ),

            Pattern::Neg(p) => Expr::Neg(Box::new(p.substitute(bindings))),

            Pattern::Inv(p) => Expr::Inv(Box::new(p.substitute(bindings))),

            Pattern::Fn(kind, p) => Expr::Fn(kind.clone(), Box::new(p.substitute(bindings))),
        }
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

    /// Add a rule to the set. Duplicates are allowed.
    pub fn add(&mut self, rule: Rule) -> &mut Self {
        self.rules.push(rule);
        self
    }

    /// Merge another RuleSet into this one. Duplicates are allowed.
    pub fn merge(&mut self, other: RuleSet) -> &mut Self {
        self.rules.extend(other.rules);
        self
    }

    /// Returns the number of rules in the set.
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// Returns true if the set contains no rules.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Returns an iterator over the rules.
    pub fn iter(&self) -> impl Iterator<Item = &Rule> {
        self.rules.iter()
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

    // === substitute tests ===

    #[test]
    fn substitute_wildcard() {
        let pattern = wildcard("x");
        let mut bindings = Bindings::new();
        bindings.insert("x".to_string(), scalar("a"));
        assert_eq!(pattern.substitute(&bindings), scalar("a"));
    }

    #[test]
    fn substitute_const() {
        let pattern = Pattern::Const(42.0);
        let bindings = Bindings::new();
        assert_eq!(pattern.substitute(&bindings), constant(42.0));
    }

    #[test]
    fn substitute_add() {
        // Pattern: x + y
        let pattern = p_add(wildcard("x"), wildcard("y"));
        let mut bindings = Bindings::new();
        bindings.insert("x".to_string(), scalar("a"));
        bindings.insert("y".to_string(), scalar("b"));
        assert_eq!(pattern.substitute(&bindings), add(scalar("a"), scalar("b")));
    }

    #[test]
    fn substitute_nested() {
        // Pattern: (x + y) * z
        let pattern = p_mul(p_add(wildcard("x"), wildcard("y")), wildcard("z"));
        let mut bindings = Bindings::new();
        bindings.insert("x".to_string(), constant(1.0));
        bindings.insert("y".to_string(), constant(2.0));
        bindings.insert("z".to_string(), scalar("a"));
        assert_eq!(
            pattern.substitute(&bindings),
            mul(add(constant(1.0), constant(2.0)), scalar("a"))
        );
    }

    #[test]
    fn substitute_repeated_wildcard() {
        // Pattern: x + x (same wildcard used twice)
        let pattern = p_add(wildcard("x"), wildcard("x"));
        let mut bindings = Bindings::new();
        bindings.insert("x".to_string(), scalar("a"));
        // Both x's get the same value
        assert_eq!(pattern.substitute(&bindings), add(scalar("a"), scalar("a")));
    }

    #[test]
    fn match_then_substitute_roundtrip() {
        // Pattern: x + y
        let pattern = p_add(wildcard("x"), wildcard("y"));
        // Expression: a + b
        let expr = add(scalar("a"), scalar("b"));

        // Match to get bindings
        let bindings = pattern.match_expr(&expr).unwrap();
        // Substitute back - should get original expression
        let result = pattern.substitute(&bindings);
        assert_eq!(result, expr);
    }

    // === RuleSet tests ===

    fn make_rule(name: &str) -> Rule {
        Rule {
            name: name.to_string(),
            lhs: wildcard("x"),
            rhs: wildcard("x"),
        }
    }

    #[test]
    fn ruleset_add() {
        let mut rs = RuleSet::new();
        assert!(rs.is_empty());

        rs.add(make_rule("r1"));
        assert_eq!(rs.len(), 1);

        rs.add(make_rule("r2"));
        assert_eq!(rs.len(), 2);
    }

    #[test]
    fn ruleset_add_allows_duplicates() {
        let mut rs = RuleSet::new();
        rs.add(make_rule("r1"));
        rs.add(make_rule("r1")); // same name
        assert_eq!(rs.len(), 2);
    }

    #[test]
    fn ruleset_merge() {
        let mut rs1 = RuleSet::new();
        rs1.add(make_rule("r1"));
        rs1.add(make_rule("r2"));

        let mut rs2 = RuleSet::new();
        rs2.add(make_rule("r3"));
        rs2.add(make_rule("r4"));

        rs1.merge(rs2);
        assert_eq!(rs1.len(), 4);
    }

    #[test]
    fn ruleset_chaining() {
        let mut rs = RuleSet::new();
        rs.add(make_rule("r1"))
            .add(make_rule("r2"))
            .add(make_rule("r3"));
        assert_eq!(rs.len(), 3);
    }
}
