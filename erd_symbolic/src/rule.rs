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

// Pattern builder functions
pub fn wildcard(name: &str) -> Pattern {
    Pattern::Wildcard(name.to_string())
}

pub fn const_wild(name: &str) -> Pattern {
    Pattern::ConstWild(name.to_string())
}

pub fn p_const(n: f64) -> Pattern {
    Pattern::Const(n)
}

pub fn p_add(a: Pattern, b: Pattern) -> Pattern {
    Pattern::Add(Box::new(a), Box::new(b))
}

pub fn p_mul(a: Pattern, b: Pattern) -> Pattern {
    Pattern::Mul(Box::new(a), Box::new(b))
}

pub fn p_neg(a: Pattern) -> Pattern {
    Pattern::Neg(Box::new(a))
}

pub fn p_inv(a: Pattern) -> Pattern {
    Pattern::Inv(Box::new(a))
}

pub fn p_pow(base: Pattern, exp: Pattern) -> Pattern {
    Pattern::Pow(Box::new(base), Box::new(exp))
}

pub fn p_sin(a: Pattern) -> Pattern {
    Pattern::Fn(FnKind::Sin, Box::new(a))
}

pub fn p_cos(a: Pattern) -> Pattern {
    Pattern::Fn(FnKind::Cos, Box::new(a))
}

pub fn p_exp(a: Pattern) -> Pattern {
    Pattern::Fn(FnKind::Exp, Box::new(a))
}

pub fn p_ln(a: Pattern) -> Pattern {
    Pattern::Fn(FnKind::Ln, Box::new(a))
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
    /// Apply rule left-to-right: match lhs, substitute into rhs.
    pub fn apply_ltr(&self, expr: &Expr) -> Option<Expr> {
        let bindings = self.lhs.match_expr(expr)?;
        Some(self.rhs.substitute(&bindings))
    }

    /// Apply rule right-to-left: match rhs, substitute into lhs.
    pub fn apply_rtl(&self, expr: &Expr) -> Option<Expr> {
        let bindings = self.rhs.match_expr(expr)?;
        Some(self.lhs.substitute(&bindings))
    }
}

pub struct RuleSet {
    rules: Vec<Rule>,
}

/// Helper to create a rule.
pub fn rule(name: &str, lhs: Pattern, rhs: Pattern) -> Rule {
    Rule {
        name: name.to_string(),
        lhs,
        rhs,
    }
}

impl RuleSet {
    /// Standard arithmetic identities.
    pub fn standard() -> RuleSet {
        let x = || wildcard("x");

        let mut rs = Self::new();

        // Additive identity: x + 0 = x
        rs.add(rule("add_zero_right", p_add(x(), p_const(0.0)), x()));
        rs.add(rule("add_zero_left", p_add(p_const(0.0), x()), x()));

        // Multiplicative identity: x * 1 = x
        rs.add(rule("mul_one_right", p_mul(x(), p_const(1.0)), x()));
        rs.add(rule("mul_one_left", p_mul(p_const(1.0), x()), x()));

        // Multiplicative zero: x * 0 = 0
        rs.add(rule("mul_zero_right", p_mul(x(), p_const(0.0)), p_const(0.0)));
        rs.add(rule("mul_zero_left", p_mul(p_const(0.0), x()), p_const(0.0)));

        // Double negation: --x = x
        rs.add(rule("double_neg", p_neg(p_neg(x())), x()));

        // Negation of zero: -0 = 0
        rs.add(rule("neg_zero", p_neg(p_const(0.0)), p_const(0.0)));

        // Double inverse: 1/(1/x) = x
        rs.add(rule("double_inv", p_inv(p_inv(x())), x()));

        // Power identities
        rs.add(rule("pow_one", p_pow(x(), p_const(1.0)), x())); // x^1 = x
        rs.add(rule("pow_zero", p_pow(x(), p_const(0.0)), p_const(1.0))); // x^0 = 1
        rs.add(rule("one_pow", p_pow(p_const(1.0), x()), p_const(1.0))); // 1^x = 1

        rs
    }

    /// Trigonometric identities.
    pub fn trigonometric() -> RuleSet {
        use std::f64::consts::{FRAC_PI_2, PI};

        let x = || wildcard("x");

        let mut rs = Self::new();

        // === Evaluation at special values ===
        rs.add(rule("sin_zero", p_sin(p_const(0.0)), p_const(0.0))); // sin(0) = 0
        rs.add(rule("cos_zero", p_cos(p_const(0.0)), p_const(1.0))); // cos(0) = 1
        rs.add(rule("sin_pi", p_sin(p_const(PI)), p_const(0.0))); // sin(π) = 0
        rs.add(rule("cos_pi", p_cos(p_const(PI)), p_const(-1.0))); // cos(π) = -1
        rs.add(rule("sin_pi_2", p_sin(p_const(FRAC_PI_2)), p_const(1.0))); // sin(π/2) = 1
        rs.add(rule("cos_pi_2", p_cos(p_const(FRAC_PI_2)), p_const(0.0))); // cos(π/2) = 0

        // === Parity (odd/even functions) ===
        // sin(-x) = -sin(x)
        rs.add(rule("sin_neg", p_sin(p_neg(x())), p_neg(p_sin(x()))));
        // cos(-x) = cos(x)
        rs.add(rule("cos_neg", p_cos(p_neg(x())), p_cos(x())));

        // === Pythagorean identity ===
        // sin²x + cos²x = 1
        rs.add(rule(
            "pythagorean",
            p_add(
                p_pow(p_sin(x()), p_const(2.0)),
                p_pow(p_cos(x()), p_const(2.0)),
            ),
            p_const(1.0),
        ));
        // sin²x = 1 - cos²x
        rs.add(rule(
            "sin_sq_from_cos",
            p_pow(p_sin(x()), p_const(2.0)),
            p_add(p_const(1.0), p_neg(p_pow(p_cos(x()), p_const(2.0)))),
        ));
        // cos²x = 1 - sin²x
        rs.add(rule(
            "cos_sq_from_sin",
            p_pow(p_cos(x()), p_const(2.0)),
            p_add(p_const(1.0), p_neg(p_pow(p_sin(x()), p_const(2.0)))),
        ));

        // === Double angle (useful for simplification) ===
        // sin(2x) = 2 sin(x) cos(x) — represented with repeated wildcard
        // Note: Matching 2*x requires a different pattern; these are structural rules

        // === Exponential/Log identities ===
        rs.add(rule("exp_zero", p_exp(p_const(0.0)), p_const(1.0))); // e^0 = 1
        rs.add(rule("ln_one", p_ln(p_const(1.0)), p_const(0.0))); // ln(1) = 0
        rs.add(rule("exp_ln", p_exp(p_ln(x())), x())); // e^(ln(x)) = x
        rs.add(rule("ln_exp", p_ln(p_exp(x())), x())); // ln(e^x) = x

        rs
    }

    /// Tensor algebra rules.
    ///
    /// Note: Full tensor rules (Kronecker delta contraction, metric tensor operations,
    /// index symmetries) require extending Pattern with index-aware matching.
    /// Currently, this provides algebraic rules useful in tensor calculations.
    ///
    /// Future extensions needed for complete tensor support:
    /// - Pattern::Var { name, indices } for matching specific tensor structures
    /// - Index pattern matching (upper/lower, specific names, wildcards)
    /// - Kronecker delta: δ^i_j v^j = v^i
    /// - Metric tensor: g^{ij} g_{jk} = δ^i_k
    /// - Symmetry annotations: A^{ij} = A^{ji} for symmetric tensors
    pub fn tensor() -> RuleSet {
        let x = || wildcard("x");
        let y = || wildcard("y");
        let a = || wildcard("a");
        let b = || wildcard("b");

        let mut rs = Self::new();

        // === Power laws (useful for tensor index manipulation) ===
        // x^a * x^b = x^(a+b)
        rs.add(rule(
            "pow_mul_same_base",
            p_mul(p_pow(x(), a()), p_pow(x(), b())),
            p_pow(x(), p_add(a(), b())),
        ));

        // (x^a)^b = x^(a*b)
        rs.add(rule(
            "pow_pow",
            p_pow(p_pow(x(), a()), b()),
            p_pow(x(), p_mul(a(), b())),
        ));

        // (x * y)^a = x^a * y^a
        rs.add(rule(
            "pow_mul_distribute",
            p_pow(p_mul(x(), y()), a()),
            p_mul(p_pow(x(), a()), p_pow(y(), a())),
        ));

        // (x / y)^a = x^a / y^a (represented as x^a * (1/y)^a)
        rs.add(rule(
            "pow_div_distribute",
            p_pow(p_mul(x(), p_inv(y())), a()),
            p_mul(p_pow(x(), a()), p_pow(p_inv(y()), a())),
        ));

        // x^(-a) = 1/x^a
        rs.add(rule(
            "pow_neg_exp",
            p_pow(x(), p_neg(a())),
            p_inv(p_pow(x(), a())),
        ));

        // === Inverse distribution ===
        // 1/(x * y) = (1/x) * (1/y)
        rs.add(rule(
            "inv_mul_distribute",
            p_inv(p_mul(x(), y())),
            p_mul(p_inv(x()), p_inv(y())),
        ));

        // === Multiplication associativity (for reordering) ===
        // (x * y) * z = x * (y * z)
        rs.add(rule(
            "mul_assoc_right",
            p_mul(p_mul(x(), y()), wildcard("z")),
            p_mul(x(), p_mul(y(), wildcard("z"))),
        ));

        // x * (y * z) = (x * y) * z
        rs.add(rule(
            "mul_assoc_left",
            p_mul(x(), p_mul(y(), wildcard("z"))),
            p_mul(p_mul(x(), y()), wildcard("z")),
        ));

        // === Addition associativity ===
        // (x + y) + z = x + (y + z)
        rs.add(rule(
            "add_assoc_right",
            p_add(p_add(x(), y()), wildcard("z")),
            p_add(x(), p_add(y(), wildcard("z"))),
        ));

        // x + (y + z) = (x + y) + z
        rs.add(rule(
            "add_assoc_left",
            p_add(x(), p_add(y(), wildcard("z"))),
            p_add(p_add(x(), y()), wildcard("z")),
        ));

        // === Distribution (for expanding/factoring) ===
        // x * (y + z) = x*y + x*z
        rs.add(rule(
            "distribute_left",
            p_mul(x(), p_add(y(), wildcard("z"))),
            p_add(p_mul(x(), y()), p_mul(x(), wildcard("z"))),
        ));

        // (x + y) * z = x*z + y*z
        rs.add(rule(
            "distribute_right",
            p_mul(p_add(x(), y()), wildcard("z")),
            p_add(p_mul(x(), wildcard("z")), p_mul(y(), wildcard("z"))),
        ));

        // === Negation distribution ===
        // -(x + y) = -x + -y
        rs.add(rule(
            "neg_add_distribute",
            p_neg(p_add(x(), y())),
            p_add(p_neg(x()), p_neg(y())),
        ));

        // -(x * y) = -x * y = x * -y (choose first form)
        rs.add(rule("neg_mul", p_neg(p_mul(x(), y())), p_mul(p_neg(x()), y())));

        rs
    }

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
    use crate::expr::{add, constant, cos, exp, inv, ln, mul, neg, scalar, sin};
    use crate::pow;

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

    // === Rule apply tests ===

    #[test]
    fn apply_ltr_identity() {
        // Rule: x + 0 -> x (additive identity)
        let rule = Rule {
            name: "add_zero".to_string(),
            lhs: p_add(wildcard("x"), Pattern::Const(0.0)),
            rhs: wildcard("x"),
        };

        // Expression: a + 0
        let expr = add(scalar("a"), constant(0.0));
        let result = rule.apply_ltr(&expr).unwrap();
        assert_eq!(result, scalar("a"));
    }

    #[test]
    fn apply_ltr_no_match() {
        // Rule: x + 0 -> x
        let rule = Rule {
            name: "add_zero".to_string(),
            lhs: p_add(wildcard("x"), Pattern::Const(0.0)),
            rhs: wildcard("x"),
        };

        // Expression: a + b (doesn't match x + 0)
        let expr = add(scalar("a"), scalar("b"));
        assert!(rule.apply_ltr(&expr).is_none());
    }

    #[test]
    fn apply_rtl_identity() {
        // Rule: x + 0 -> x (apply in reverse: x -> x + 0)
        let rule = Rule {
            name: "add_zero".to_string(),
            lhs: p_add(wildcard("x"), Pattern::Const(0.0)),
            rhs: wildcard("x"),
        };

        // Expression: a (matches rhs pattern "x")
        let expr = scalar("a");
        let result = rule.apply_rtl(&expr).unwrap();
        assert_eq!(result, add(scalar("a"), constant(0.0)));
    }

    #[test]
    fn apply_ltr_double_negation() {
        // Rule: --x -> x (double negation elimination)
        let rule = Rule {
            name: "double_neg".to_string(),
            lhs: p_neg(p_neg(wildcard("x"))),
            rhs: wildcard("x"),
        };

        // Expression: --a
        let expr = neg(neg(scalar("a")));
        let result = rule.apply_ltr(&expr).unwrap();
        assert_eq!(result, scalar("a"));
    }

    #[test]
    fn apply_rtl_double_negation() {
        // Rule: --x -> x (apply in reverse: x -> --x)
        let rule = Rule {
            name: "double_neg".to_string(),
            lhs: p_neg(p_neg(wildcard("x"))),
            rhs: wildcard("x"),
        };

        // Expression: a
        let expr = scalar("a");
        let result = rule.apply_rtl(&expr).unwrap();
        assert_eq!(result, neg(neg(scalar("a"))));
    }

    #[test]
    fn apply_ltr_with_repeated_wildcard() {
        // Rule: x * x -> x^2 (squaring)
        let rule = Rule {
            name: "square".to_string(),
            lhs: p_mul(wildcard("x"), wildcard("x")),
            rhs: Pattern::Pow(Box::new(wildcard("x")), Box::new(Pattern::Const(2.0))),
        };

        // Expression: a * a
        let expr = mul(scalar("a"), scalar("a"));
        let result = rule.apply_ltr(&expr).unwrap();
        assert_eq!(result, pow(scalar("a"), constant(2.0)));
    }

    #[test]
    fn apply_ltr_repeated_wildcard_no_match() {
        // Rule: x * x -> x^2
        let rule = Rule {
            name: "square".to_string(),
            lhs: p_mul(wildcard("x"), wildcard("x")),
            rhs: Pattern::Pow(Box::new(wildcard("x")), Box::new(Pattern::Const(2.0))),
        };

        // Expression: a * b (different operands, won't match x * x)
        let expr = mul(scalar("a"), scalar("b"));
        assert!(rule.apply_ltr(&expr).is_none());
    }

    #[test]
    fn apply_ltr_nested() {
        // Rule: (x + y) * z -> x*z + y*z (distribution, simplified)
        let rule = Rule {
            name: "distribute".to_string(),
            lhs: p_mul(p_add(wildcard("x"), wildcard("y")), wildcard("z")),
            rhs: p_add(
                p_mul(wildcard("x"), wildcard("z")),
                p_mul(wildcard("y"), wildcard("z")),
            ),
        };

        // Expression: (a + b) * c
        let expr = mul(add(scalar("a"), scalar("b")), scalar("c"));
        let result = rule.apply_ltr(&expr).unwrap();
        assert_eq!(
            result,
            add(mul(scalar("a"), scalar("c")), mul(scalar("b"), scalar("c")))
        );
    }

    #[test]
    fn apply_rtl_factoring() {
        // Rule: (x + y) * z -> x*z + y*z (apply in reverse to factor)
        let rule = Rule {
            name: "distribute".to_string(),
            lhs: p_mul(p_add(wildcard("x"), wildcard("y")), wildcard("z")),
            rhs: p_add(
                p_mul(wildcard("x"), wildcard("z")),
                p_mul(wildcard("y"), wildcard("z")),
            ),
        };

        // Expression: a*c + b*c (matches rhs pattern)
        let expr = add(mul(scalar("a"), scalar("c")), mul(scalar("b"), scalar("c")));
        let result = rule.apply_rtl(&expr).unwrap();
        assert_eq!(result, mul(add(scalar("a"), scalar("b")), scalar("c")));
    }

    // === Standard RuleSet tests ===

    #[test]
    fn standard_ruleset_has_rules() {
        let rs = RuleSet::standard();
        assert!(!rs.is_empty());
        assert!(rs.len() >= 10); // We have at least 10 standard rules
    }

    #[test]
    fn standard_add_zero() {
        let rs = RuleSet::standard();
        let expr = add(scalar("a"), constant(0.0));

        // Find and apply the add_zero_right rule
        let result = rs
            .iter()
            .find(|r| r.name == "add_zero_right")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(scalar("a")));
    }

    #[test]
    fn standard_mul_one() {
        let rs = RuleSet::standard();
        let expr = mul(scalar("a"), constant(1.0));

        let result = rs
            .iter()
            .find(|r| r.name == "mul_one_right")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(scalar("a")));
    }

    #[test]
    fn standard_mul_zero() {
        let rs = RuleSet::standard();
        let expr = mul(scalar("a"), constant(0.0));

        let result = rs
            .iter()
            .find(|r| r.name == "mul_zero_right")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(constant(0.0)));
    }

    #[test]
    fn standard_double_neg() {
        let rs = RuleSet::standard();
        let expr = neg(neg(scalar("a")));

        let result = rs
            .iter()
            .find(|r| r.name == "double_neg")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(scalar("a")));
    }

    #[test]
    fn standard_double_inv() {
        let rs = RuleSet::standard();
        let expr = inv(inv(scalar("a")));

        let result = rs
            .iter()
            .find(|r| r.name == "double_inv")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(scalar("a")));
    }

    #[test]
    fn standard_pow_one() {
        let rs = RuleSet::standard();
        let expr = pow(scalar("a"), constant(1.0));

        let result = rs
            .iter()
            .find(|r| r.name == "pow_one")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(scalar("a")));
    }

    #[test]
    fn standard_pow_zero() {
        let rs = RuleSet::standard();
        let expr = pow(scalar("a"), constant(0.0));

        let result = rs
            .iter()
            .find(|r| r.name == "pow_zero")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(constant(1.0)));
    }

    // === Trigonometric RuleSet tests ===

    #[test]
    fn trig_ruleset_has_rules() {
        let rs = RuleSet::trigonometric();
        assert!(!rs.is_empty());
        assert!(rs.len() >= 10); // We have at least 10 trig rules
    }

    #[test]
    fn trig_sin_zero() {
        let rs = RuleSet::trigonometric();
        let expr = sin(constant(0.0));

        let result = rs
            .iter()
            .find(|r| r.name == "sin_zero")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(constant(0.0)));
    }

    #[test]
    fn trig_cos_zero() {
        let rs = RuleSet::trigonometric();
        let expr = cos(constant(0.0));

        let result = rs
            .iter()
            .find(|r| r.name == "cos_zero")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(constant(1.0)));
    }

    #[test]
    fn trig_sin_neg() {
        let rs = RuleSet::trigonometric();
        // sin(-a)
        let expr = sin(neg(scalar("a")));

        let result = rs
            .iter()
            .find(|r| r.name == "sin_neg")
            .and_then(|r| r.apply_ltr(&expr));

        // Should become -sin(a)
        assert_eq!(result, Some(neg(sin(scalar("a")))));
    }

    #[test]
    fn trig_cos_neg() {
        let rs = RuleSet::trigonometric();
        // cos(-a)
        let expr = cos(neg(scalar("a")));

        let result = rs
            .iter()
            .find(|r| r.name == "cos_neg")
            .and_then(|r| r.apply_ltr(&expr));

        // Should become cos(a)
        assert_eq!(result, Some(cos(scalar("a"))));
    }

    #[test]
    fn trig_pythagorean() {
        let rs = RuleSet::trigonometric();
        // sin²(a) + cos²(a)
        let expr = add(
            pow(sin(scalar("a")), constant(2.0)),
            pow(cos(scalar("a")), constant(2.0)),
        );

        let result = rs
            .iter()
            .find(|r| r.name == "pythagorean")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(constant(1.0)));
    }

    #[test]
    fn trig_exp_ln() {
        let rs = RuleSet::trigonometric();
        // exp(ln(a))
        let expr = exp(ln(scalar("a")));

        let result = rs
            .iter()
            .find(|r| r.name == "exp_ln")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(scalar("a")));
    }

    #[test]
    fn trig_ln_exp() {
        let rs = RuleSet::trigonometric();
        // ln(exp(a))
        let expr = ln(exp(scalar("a")));

        let result = rs
            .iter()
            .find(|r| r.name == "ln_exp")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(scalar("a")));
    }

    // === Tensor RuleSet tests ===

    #[test]
    fn tensor_ruleset_has_rules() {
        let rs = RuleSet::tensor();
        assert!(!rs.is_empty());
        assert!(rs.len() >= 10); // We have at least 10 tensor rules
    }

    #[test]
    fn tensor_pow_mul_same_base() {
        let rs = RuleSet::tensor();
        // x^2 * x^3 = x^(2+3)
        let expr = mul(
            pow(scalar("x"), constant(2.0)),
            pow(scalar("x"), constant(3.0)),
        );

        let result = rs
            .iter()
            .find(|r| r.name == "pow_mul_same_base")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(
            result,
            Some(pow(scalar("x"), add(constant(2.0), constant(3.0))))
        );
    }

    #[test]
    fn tensor_pow_pow() {
        let rs = RuleSet::tensor();
        // (x^2)^3 = x^(2*3)
        let expr = pow(pow(scalar("x"), constant(2.0)), constant(3.0));

        let result = rs
            .iter()
            .find(|r| r.name == "pow_pow")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(
            result,
            Some(pow(scalar("x"), mul(constant(2.0), constant(3.0))))
        );
    }

    #[test]
    fn tensor_pow_mul_distribute() {
        let rs = RuleSet::tensor();
        // (x * y)^2 = x^2 * y^2
        let expr = pow(mul(scalar("x"), scalar("y")), constant(2.0));

        let result = rs
            .iter()
            .find(|r| r.name == "pow_mul_distribute")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(
            result,
            Some(mul(
                pow(scalar("x"), constant(2.0)),
                pow(scalar("y"), constant(2.0))
            ))
        );
    }

    #[test]
    fn tensor_pow_neg_exp() {
        let rs = RuleSet::tensor();
        // x^(-2) = 1/x^2
        let expr = pow(scalar("x"), neg(constant(2.0)));

        let result = rs
            .iter()
            .find(|r| r.name == "pow_neg_exp")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(inv(pow(scalar("x"), constant(2.0)))));
    }

    #[test]
    fn tensor_inv_mul_distribute() {
        let rs = RuleSet::tensor();
        // 1/(x * y) = (1/x) * (1/y)
        let expr = inv(mul(scalar("x"), scalar("y")));

        let result = rs
            .iter()
            .find(|r| r.name == "inv_mul_distribute")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(mul(inv(scalar("x")), inv(scalar("y")))));
    }

    #[test]
    fn tensor_distribute_left() {
        let rs = RuleSet::tensor();
        // a * (b + c) = a*b + a*c
        let expr = mul(scalar("a"), add(scalar("b"), scalar("c")));

        let result = rs
            .iter()
            .find(|r| r.name == "distribute_left")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(
            result,
            Some(add(
                mul(scalar("a"), scalar("b")),
                mul(scalar("a"), scalar("c"))
            ))
        );
    }

    #[test]
    fn tensor_mul_assoc() {
        let rs = RuleSet::tensor();
        // (a * b) * c = a * (b * c)
        let expr = mul(mul(scalar("a"), scalar("b")), scalar("c"));

        let result = rs
            .iter()
            .find(|r| r.name == "mul_assoc_right")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(
            result,
            Some(mul(scalar("a"), mul(scalar("b"), scalar("c"))))
        );
    }

    #[test]
    fn tensor_neg_add_distribute() {
        let rs = RuleSet::tensor();
        // -(a + b) = -a + -b
        let expr = neg(add(scalar("a"), scalar("b")));

        let result = rs
            .iter()
            .find(|r| r.name == "neg_add_distribute")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(add(neg(scalar("a")), neg(scalar("b")))));
    }
}
