use crate::expr::{Expr, FnKind, Index, IndexPosition};
use std::collections::HashMap;

/// Pattern for matching index names in tensor expressions.
#[derive(Debug, Clone)]
pub enum IndexPattern {
    /// Match an upper (contravariant) index, binding its name to the wildcard.
    Upper(String),
    /// Match a lower (covariant) index, binding its name to the wildcard.
    Lower(String),
    /// Match an index with exact name and position.
    Exact(Index),
}

/// Pattern for matching variable names.
#[derive(Debug, Clone)]
pub enum VarPattern {
    /// Match a variable with this exact name.
    Exact(String),
    /// Match any variable, binding its name to this wildcard.
    Wild(String),
}

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
    FnN(FnKind, Vec<Pattern>),
    /// Match a variable with specific name pattern and index structure.
    Var {
        name: VarPattern,
        indices: Vec<IndexPattern>,
    },
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

pub fn p_tan(a: Pattern) -> Pattern {
    Pattern::Fn(FnKind::Tan, Box::new(a))
}

pub fn p_asin(a: Pattern) -> Pattern {
    Pattern::Fn(FnKind::Asin, Box::new(a))
}

pub fn p_acos(a: Pattern) -> Pattern {
    Pattern::Fn(FnKind::Acos, Box::new(a))
}

pub fn p_atan(a: Pattern) -> Pattern {
    Pattern::Fn(FnKind::Atan, Box::new(a))
}

pub fn p_sign(a: Pattern) -> Pattern {
    Pattern::Fn(FnKind::Sign, Box::new(a))
}

pub fn p_sinh(a: Pattern) -> Pattern {
    Pattern::Fn(FnKind::Sinh, Box::new(a))
}

pub fn p_cosh(a: Pattern) -> Pattern {
    Pattern::Fn(FnKind::Cosh, Box::new(a))
}

pub fn p_tanh(a: Pattern) -> Pattern {
    Pattern::Fn(FnKind::Tanh, Box::new(a))
}

pub fn p_floor(a: Pattern) -> Pattern {
    Pattern::Fn(FnKind::Floor, Box::new(a))
}

pub fn p_ceil(a: Pattern) -> Pattern {
    Pattern::Fn(FnKind::Ceil, Box::new(a))
}

pub fn p_round(a: Pattern) -> Pattern {
    Pattern::Fn(FnKind::Round, Box::new(a))
}

pub fn p_min(a: Pattern, b: Pattern) -> Pattern {
    Pattern::FnN(FnKind::Min, vec![a, b])
}

pub fn p_max(a: Pattern, b: Pattern) -> Pattern {
    Pattern::FnN(FnKind::Max, vec![a, b])
}

pub fn p_clamp(x: Pattern, lo: Pattern, hi: Pattern) -> Pattern {
    Pattern::FnN(FnKind::Clamp, vec![x, lo, hi])
}

pub fn p_exp(a: Pattern) -> Pattern {
    Pattern::Fn(FnKind::Exp, Box::new(a))
}

pub fn p_ln(a: Pattern) -> Pattern {
    Pattern::Fn(FnKind::Ln, Box::new(a))
}

// === Var pattern builders ===

/// Create a pattern matching a variable with exact name and index patterns.
pub fn p_var(name: &str, indices: Vec<IndexPattern>) -> Pattern {
    Pattern::Var {
        name: VarPattern::Exact(name.to_string()),
        indices,
    }
}

/// Create a pattern matching any variable name (binding to wildcard) with index patterns.
pub fn p_var_wild(name_wildcard: &str, indices: Vec<IndexPattern>) -> Pattern {
    Pattern::Var {
        name: VarPattern::Wild(name_wildcard.to_string()),
        indices,
    }
}

/// Create an upper index pattern that binds the index name to a wildcard.
pub fn idx_upper(wildcard: &str) -> IndexPattern {
    IndexPattern::Upper(wildcard.to_string())
}

/// Create a lower index pattern that binds the index name to a wildcard.
pub fn idx_lower(wildcard: &str) -> IndexPattern {
    IndexPattern::Lower(wildcard.to_string())
}

/// Create an exact index pattern.
pub fn idx_exact(name: &str, position: IndexPosition) -> IndexPattern {
    IndexPattern::Exact(Index {
        name: name.to_string(),
        position,
    })
}

pub struct Rule {
    pub name: String,
    pub lhs: Pattern,
    pub rhs: Pattern,
}

/// Expression bindings: wildcard name -> matched expression
pub type ExprBindings = HashMap<String, Expr>;

/// Index bindings: wildcard name -> matched index name
pub type IndexBindings = HashMap<String, String>;

/// Combined bindings from pattern matching.
#[derive(Debug, Clone, Default)]
pub struct Bindings {
    pub exprs: ExprBindings,
    pub indices: IndexBindings,
}

impl Bindings {
    pub fn new() -> Self {
        Bindings {
            exprs: ExprBindings::new(),
            indices: IndexBindings::new(),
        }
    }
}

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
            (Pattern::Wildcard(name), _) => bind_expr(name, expr.clone(), bindings),

            // ConstWild matches only constants
            (Pattern::ConstWild(name), Expr::Const(_)) => bind_expr(name, expr.clone(), bindings),
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

            (Pattern::FnN(pk, ps), Expr::FnN(ek, args)) if pk == ek => {
                if ps.len() != args.len() {
                    return false;
                }
                ps.iter()
                    .zip(args.iter())
                    .all(|(p, a)| p.match_expr_inner(a, bindings))
            }
            (Pattern::FnN(_, _), _) => false,

            // Variable matching with index patterns
            (
                Pattern::Var {
                    name: pat_name,
                    indices: pat_indices,
                },
                Expr::Var {
                    name: expr_name,
                    indices: expr_indices,
                },
            ) => {
                // Check variable name matches
                let name_matches = match pat_name {
                    VarPattern::Exact(pn) => pn == expr_name,
                    VarPattern::Wild(wildcard) => bind_expr(wildcard, expr.clone(), bindings),
                };
                if !name_matches {
                    return false;
                }

                // Check index count matches
                if pat_indices.len() != expr_indices.len() {
                    return false;
                }

                // Check each index pattern
                for (pat_idx, expr_idx) in pat_indices.iter().zip(expr_indices.iter()) {
                    let idx_matches = match pat_idx {
                        IndexPattern::Upper(wildcard) => {
                            expr_idx.position == IndexPosition::Upper
                                && bind_index(wildcard, &expr_idx.name, bindings)
                        }
                        IndexPattern::Lower(wildcard) => {
                            expr_idx.position == IndexPosition::Lower
                                && bind_index(wildcard, &expr_idx.name, bindings)
                        }
                        IndexPattern::Exact(expected) => {
                            expr_idx.name == expected.name && expr_idx.position == expected.position
                        }
                    };
                    if !idx_matches {
                        return false;
                    }
                }
                true
            }
            (Pattern::Var { .. }, _) => false,
        }
    }

    /// Substitute bindings into this pattern to produce an expression.
    /// Panics if a wildcard is not found in bindings.
    pub fn substitute(&self, bindings: &Bindings) -> Expr {
        match self {
            Pattern::Wildcard(name) | Pattern::ConstWild(name) => bindings
                .exprs
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
            Pattern::FnN(kind, args) => Expr::FnN(
                kind.clone(),
                args.iter().map(|a| a.substitute(bindings)).collect(),
            ),

            Pattern::Var {
                name: pat_name,
                indices: pat_indices,
            } => {
                // Resolve variable name
                let var_name = match pat_name {
                    VarPattern::Exact(n) => n.clone(),
                    VarPattern::Wild(wildcard) => {
                        // Get the name from the bound expression (must be a Var)
                        match bindings.exprs.get(wildcard) {
                            Some(Expr::Var { name, .. }) => name.clone(),
                            Some(_) => {
                                panic!("Var wildcard {} bound to non-Var expression", wildcard)
                            }
                            None => panic!("Unbound var wildcard: {}", wildcard),
                        }
                    }
                };

                // Resolve indices
                let var_indices: Vec<Index> = pat_indices
                    .iter()
                    .map(|pat_idx| match pat_idx {
                        IndexPattern::Upper(wildcard) => Index {
                            name: bindings
                                .indices
                                .get(wildcard)
                                .unwrap_or_else(|| panic!("Unbound index wildcard: {}", wildcard))
                                .clone(),
                            position: IndexPosition::Upper,
                        },
                        IndexPattern::Lower(wildcard) => Index {
                            name: bindings
                                .indices
                                .get(wildcard)
                                .unwrap_or_else(|| panic!("Unbound index wildcard: {}", wildcard))
                                .clone(),
                            position: IndexPosition::Lower,
                        },
                        IndexPattern::Exact(idx) => idx.clone(),
                    })
                    .collect();

                Expr::Var {
                    name: var_name,
                    indices: var_indices,
                }
            }
        }
    }
}

/// Bind a name to an expression, checking consistency with existing bindings.
fn bind_expr(name: &str, expr: Expr, bindings: &mut Bindings) -> bool {
    match bindings.exprs.get(name) {
        Some(existing) => *existing == expr, // Must match existing binding
        None => {
            bindings.exprs.insert(name.to_string(), expr);
            true
        }
    }
}

/// Bind an index wildcard to an index name, checking consistency.
fn bind_index(wildcard: &str, index_name: &str, bindings: &mut Bindings) -> bool {
    match bindings.indices.get(wildcard) {
        Some(existing) => existing == index_name, // Must match existing binding
        None => {
            bindings
                .indices
                .insert(wildcard.to_string(), index_name.to_string());
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
    pub fn new() -> Self {
        RuleSet { rules: Vec::new() }
    }

    pub fn full() -> Self {
        let mut rules = Self::new();
        rules
            .merge(Self::standard())
            .merge(Self::trigonometric())
            .merge(Self::tensor());
        rules
    }

    /// Extended identities that assume a numeric domain with ordering/rounding semantics.
    /// These are not always safe for symbolic manipulation without domain constraints.
    pub fn extended() -> RuleSet {
        let x = || wildcard("x");
        let y = || wildcard("y");
        let lo = || wildcard("lo");
        let hi = || wildcard("hi");

        let mut rs = Self::new();

        // Min/max commutativity
        rs.add(rule("min_commute", p_min(x(), y()), p_min(y(), x())));
        rs.add(rule("max_commute", p_max(x(), y()), p_max(y(), x())));

        // Absorption
        rs.add(rule(
            "min_absorb",
            p_min(x(), p_max(x(), y())),
            x(),
        ));
        rs.add(rule(
            "max_absorb",
            p_max(x(), p_min(x(), y())),
            x(),
        ));

        // min(x, y) + max(x, y) = x + y
        rs.add(rule(
            "min_max_sum",
            p_add(p_min(x(), y()), p_max(x(), y())),
            p_add(x(), y()),
        ));

        // clamp(x, lo, hi) = min(max(x, lo), hi)
        rs.add(rule(
            "clamp_def",
            p_clamp(x(), lo(), hi()),
            p_min(p_max(x(), lo()), hi()),
        ));

        // sign(-x) = -sign(x)
        rs.add(rule(
            "sign_neg",
            p_sign(p_neg(x())),
            p_neg(p_sign(x())),
        ));

        // sign(x)^2 = 1 (for x != 0)
        rs.add(rule(
            "sign_square",
            p_pow(p_sign(x()), p_const(2.0)),
            p_const(1.0),
        ));

        // round(x) = floor(x + 1/2) (assumes round-half-up)
        rs.add(rule(
            "round_def",
            p_round(x()),
            p_floor(p_add(x(), p_inv(p_const(2.0)))),
        ));

        rs
    }

    /// Standard arithmetic identities.
    pub fn standard() -> RuleSet {
        let x = || wildcard("x");
        let lo = || wildcard("lo");
        let hi = || wildcard("hi");
        let v = || p_var_wild("v", vec![]);

        let mut rs = Self::new();

        // Additive identity: x + 0 = x
        rs.add(rule("add_zero_right", p_add(x(), p_const(0.0)), x()));
        rs.add(rule("add_zero_left", p_add(p_const(0.0), x()), x()));

        // Multiplicative identity: x * 1 = x
        rs.add(rule("mul_one_right", p_mul(x(), p_const(1.0)), x()));
        rs.add(rule("mul_one_left", p_mul(p_const(1.0), x()), x()));

        // Multiplicative zero: x * 0 = 0
        rs.add(rule(
            "mul_zero_right",
            p_mul(x(), p_const(0.0)),
            p_const(0.0),
        ));
        rs.add(rule(
            "mul_zero_left",
            p_mul(p_const(0.0), x()),
            p_const(0.0),
        ));

        // Double negation: --x = x
        rs.add(rule("double_neg", p_neg(p_neg(x())), x()));

        // Negation of zero: -0 = 0
        rs.add(rule("neg_zero", p_neg(p_const(0.0)), p_const(0.0)));

        // Double inverse: 1/(1/x) = x
        rs.add(rule("double_inv", p_inv(p_inv(x())), x()));

        // Inverse of one: 1/1 = 1
        rs.add(rule("inv_one", p_inv(p_const(1.0)), p_const(1.0)));

        // Power identities
        rs.add(rule("pow_one", p_pow(x(), p_const(1.0)), x())); // x^1 = x
        rs.add(rule("pow_zero", p_pow(x(), p_const(0.0)), p_const(1.0))); // x^0 = 1
        rs.add(rule("one_pow", p_pow(p_const(1.0), x()), p_const(1.0))); // 1^x = 1
        rs.add(rule("zero_pow", p_pow(p_const(0.0), x()), p_const(0.0))); // 0^x = 0 (for x > 0)

        // Term cancellation: x + (-x) = 0
        rs.add(rule(
            "add_neg_self_right",
            p_add(x(), p_neg(x())),
            p_const(0.0),
        ));
        rs.add(rule(
            "add_neg_self_left",
            p_add(p_neg(x()), x()),
            p_const(0.0),
        ));

        // Multiplication by negative one: x * (-1) = -x
        rs.add(rule(
            "mul_neg_one_right",
            p_mul(x(), p_neg(p_const(1.0))),
            p_neg(x()),
        ));
        rs.add(rule(
            "mul_neg_one_left",
            p_mul(p_neg(p_const(1.0)), x()),
            p_neg(x()),
        ));

        // Squaring: x * x = x^2
        rs.add(rule(
            "mul_self_square",
            p_mul(x(), x()),
            p_pow(x(), p_const(2.0)),
        ));

        // Targeted distribution: x * (1 + y) = x + x*y
        rs.add(rule(
            "mul_one_plus_right",
            p_mul(x(), p_add(p_const(1.0), wildcard("y"))),
            p_add(x(), p_mul(x(), wildcard("y"))),
        ));
        rs.add(rule(
            "mul_one_plus_left",
            p_mul(p_add(p_const(1.0), wildcard("y")), x()),
            p_add(x(), p_mul(x(), wildcard("y"))),
        ));

        // Targeted cancellation: x*(a+b) - x*b = x*a and x*(a+b) - x*a = x*b
        rs.add(rule(
            "mul_add_cancel_right",
            p_add(
                p_mul(x(), p_add(wildcard("a"), wildcard("b"))),
                p_neg(p_mul(x(), wildcard("b"))),
            ),
            p_mul(x(), wildcard("a")),
        ));
        rs.add(rule(
            "mul_add_cancel_left",
            p_add(
                p_mul(x(), p_add(wildcard("a"), wildcard("b"))),
                p_neg(p_mul(x(), wildcard("a"))),
            ),
            p_mul(x(), wildcard("b")),
        ));

        // Combine like terms (variables only)
        rs.add(rule(
            "combine_like_terms_left",
            p_add(p_mul(const_wild("a"), v()), v()),
            p_mul(p_add(const_wild("a"), p_const(1.0)), v()),
        ));
        rs.add(rule(
            "combine_like_terms_right",
            p_add(v(), p_mul(const_wild("a"), v())),
            p_mul(p_add(const_wild("a"), p_const(1.0)), v()),
        ));
        rs.add(rule(
            "combine_like_terms_self",
            p_add(v(), v()),
            p_mul(p_const(2.0), v()),
        ));

        // Min/max/clamp identities (idempotent)
        rs.add(rule("min_idempotent", p_min(x(), x()), x()));
        rs.add(rule("max_idempotent", p_max(x(), x()), x()));
        rs.add(rule(
            "clamp_idempotent",
            p_clamp(p_clamp(x(), lo(), hi()), lo(), hi()),
            p_clamp(x(), lo(), hi()),
        ));

        rs
    }

    /// Trigonometric identities.
    pub fn trigonometric() -> RuleSet {
        use std::f64::consts::{E, FRAC_PI_2, FRAC_PI_3, FRAC_PI_4, FRAC_PI_6, PI, TAU};

        let x = || wildcard("x");

        let mut rs = Self::new();

        // === Evaluation at special values ===
        rs.add(rule("sin_zero", p_sin(p_const(0.0)), p_const(0.0))); // sin(0) = 0
        rs.add(rule("cos_zero", p_cos(p_const(0.0)), p_const(1.0))); // cos(0) = 1
        rs.add(rule("sin_pi", p_sin(p_const(PI)), p_const(0.0))); // sin(π) = 0
        rs.add(rule("cos_pi", p_cos(p_const(PI)), p_const(-1.0))); // cos(π) = -1
        rs.add(rule("sin_pi_2", p_sin(p_const(FRAC_PI_2)), p_const(1.0))); // sin(π/2) = 1
        rs.add(rule("cos_pi_2", p_cos(p_const(FRAC_PI_2)), p_const(0.0))); // cos(π/2) = 0

        // Additional special values
        let sqrt_2_over_2 = std::f64::consts::FRAC_1_SQRT_2; // √2/2 ≈ 0.7071
        let sqrt_3_over_2 = 3.0_f64.sqrt() / 2.0; // √3/2 ≈ 0.8660
        let three_pi_over_2 = 3.0 * FRAC_PI_2; // 3π/2

        rs.add(rule("sin_pi_4", p_sin(p_const(FRAC_PI_4)), p_const(sqrt_2_over_2))); // sin(π/4) = √2/2
        rs.add(rule("cos_pi_4", p_cos(p_const(FRAC_PI_4)), p_const(sqrt_2_over_2))); // cos(π/4) = √2/2
        rs.add(rule("sin_pi_3", p_sin(p_const(FRAC_PI_3)), p_const(sqrt_3_over_2))); // sin(π/3) = √3/2
        rs.add(rule("cos_pi_3", p_cos(p_const(FRAC_PI_3)), p_const(0.5))); // cos(π/3) = 1/2
        rs.add(rule("sin_pi_6", p_sin(p_const(FRAC_PI_6)), p_const(0.5))); // sin(π/6) = 1/2
        rs.add(rule("cos_pi_6", p_cos(p_const(FRAC_PI_6)), p_const(sqrt_3_over_2))); // cos(π/6) = √3/2
        rs.add(rule("sin_2pi", p_sin(p_const(TAU)), p_const(0.0))); // sin(2π) = 0
        rs.add(rule("cos_2pi", p_cos(p_const(TAU)), p_const(1.0))); // cos(2π) = 1
        rs.add(rule("sin_3pi_2", p_sin(p_const(three_pi_over_2)), p_const(-1.0))); // sin(3π/2) = -1
        rs.add(rule("cos_3pi_2", p_cos(p_const(three_pi_over_2)), p_const(0.0))); // cos(3π/2) = 0
        rs.add(rule("tan_zero", p_tan(p_const(0.0)), p_const(0.0))); // tan(0) = 0
        rs.add(rule("tan_pi", p_tan(p_const(PI)), p_const(0.0))); // tan(π) = 0
        rs.add(rule("tan_pi_4", p_tan(p_const(FRAC_PI_4)), p_const(1.0))); // tan(π/4) = 1
        rs.add(rule("asin_zero", p_asin(p_const(0.0)), p_const(0.0))); // asin(0) = 0
        rs.add(rule("acos_one", p_acos(p_const(1.0)), p_const(0.0))); // acos(1) = 0
        rs.add(rule("acos_zero", p_acos(p_const(0.0)), p_const(FRAC_PI_2))); // acos(0) = π/2
        rs.add(rule("atan_zero", p_atan(p_const(0.0)), p_const(0.0))); // atan(0) = 0
        rs.add(rule("atan_one", p_atan(p_const(1.0)), p_const(FRAC_PI_4))); // atan(1) = π/4

        // === Parity (odd/even functions) ===
        // sin(-x) = -sin(x)
        rs.add(rule("sin_neg", p_sin(p_neg(x())), p_neg(p_sin(x()))));
        // cos(-x) = cos(x)
        rs.add(rule("cos_neg", p_cos(p_neg(x())), p_cos(x())));
        // tan(-x) = -tan(x)
        rs.add(rule("tan_neg", p_tan(p_neg(x())), p_neg(p_tan(x()))));

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

        // === Hyperbolic identities ===
        // sinh(0) = 0
        rs.add(rule("sinh_zero", p_sinh(p_const(0.0)), p_const(0.0)));
        // cosh(0) = 1
        rs.add(rule("cosh_zero", p_cosh(p_const(0.0)), p_const(1.0)));
        // tanh(0) = 0
        rs.add(rule("tanh_zero", p_tanh(p_const(0.0)), p_const(0.0)));

        // Parity: sinh(-x) = -sinh(x), cosh(-x) = cosh(x), tanh(-x) = -tanh(x)
        rs.add(rule("sinh_neg", p_sinh(p_neg(x())), p_neg(p_sinh(x()))));
        rs.add(rule("cosh_neg", p_cosh(p_neg(x())), p_cosh(x())));
        rs.add(rule("tanh_neg", p_tanh(p_neg(x())), p_neg(p_tanh(x()))));

        // cosh²x - sinh²x = 1
        rs.add(rule(
            "hyperbolic_identity",
            p_add(
                p_pow(p_cosh(x()), p_const(2.0)),
                p_neg(p_pow(p_sinh(x()), p_const(2.0))),
            ),
            p_const(1.0),
        ));

        // sinh(2x) = 2·sinh(x)·cosh(x)
        rs.add(rule(
            "sinh_double_angle",
            p_sinh(p_mul(p_const(2.0), x())),
            p_mul(p_const(2.0), p_mul(p_sinh(x()), p_cosh(x()))),
        ));
        // cosh(2x) = cosh²(x) + sinh²(x)
        rs.add(rule(
            "cosh_double_angle",
            p_cosh(p_mul(p_const(2.0), x())),
            p_add(
                p_pow(p_cosh(x()), p_const(2.0)),
                p_pow(p_sinh(x()), p_const(2.0)),
            ),
        ));

        // tanh(x) = sinh(x) / cosh(x)
        rs.add(rule(
            "tanh_def",
            p_tanh(x()),
            p_mul(p_sinh(x()), p_inv(p_cosh(x()))),
        ));
        // sinh(x) / cosh(x) = tanh(x)
        rs.add(rule(
            "tanh_def_contract",
            p_mul(p_sinh(x()), p_inv(p_cosh(x()))),
            p_tanh(x()),
        ));

        // === Angle shift identities ===

        // Complementary angles: sin(π/2 - x) = cos(x), cos(π/2 - x) = sin(x)
        // π/2 - x is represented as Add(Const(π/2), Neg(x))
        rs.add(rule(
            "sin_complementary",
            p_sin(p_add(p_const(FRAC_PI_2), p_neg(x()))),
            p_cos(x()),
        ));
        rs.add(rule(
            "sin_complementary_rev",
            p_sin(p_add(p_neg(x()), p_const(FRAC_PI_2))),
            p_cos(x()),
        ));
        rs.add(rule(
            "cos_complementary",
            p_cos(p_add(p_const(FRAC_PI_2), p_neg(x()))),
            p_sin(x()),
        ));
        rs.add(rule(
            "cos_complementary_rev",
            p_cos(p_add(p_neg(x()), p_const(FRAC_PI_2))),
            p_sin(x()),
        ));

        // Supplementary angles: sin(π - x) = sin(x), cos(π - x) = -cos(x)
        rs.add(rule(
            "sin_supplementary",
            p_sin(p_add(p_const(PI), p_neg(x()))),
            p_sin(x()),
        ));
        rs.add(rule(
            "sin_supplementary_rev",
            p_sin(p_add(p_neg(x()), p_const(PI))),
            p_sin(x()),
        ));
        rs.add(rule(
            "cos_supplementary",
            p_cos(p_add(p_const(PI), p_neg(x()))),
            p_neg(p_cos(x())),
        ));
        rs.add(rule(
            "cos_supplementary_rev",
            p_cos(p_add(p_neg(x()), p_const(PI))),
            p_neg(p_cos(x())),
        ));

        // Periodicity: sin(x + 2π) = sin(x), cos(x + 2π) = cos(x)
        rs.add(rule(
            "sin_period",
            p_sin(p_add(x(), p_const(TAU))),
            p_sin(x()),
        ));
        rs.add(rule(
            "sin_period_rev",
            p_sin(p_add(p_const(TAU), x())),
            p_sin(x()),
        ));
        rs.add(rule(
            "cos_period",
            p_cos(p_add(x(), p_const(TAU))),
            p_cos(x()),
        ));
        rs.add(rule(
            "cos_period_rev",
            p_cos(p_add(p_const(TAU), x())),
            p_cos(x()),
        ));

        // Periodicity: tan(x + π) = tan(x)
        rs.add(rule(
            "tan_period",
            p_tan(p_add(x(), p_const(PI))),
            p_tan(x()),
        ));
        rs.add(rule(
            "tan_period_rev",
            p_tan(p_add(p_const(PI), x())),
            p_tan(x()),
        ));

        // tan(x) = sin(x) / cos(x)
        rs.add(rule(
            "tan_def",
            p_tan(x()),
            p_mul(p_sin(x()), p_inv(p_cos(x()))),
        ));
        // sin(x) / cos(x) = tan(x)
        rs.add(rule(
            "tan_def_contract",
            p_mul(p_sin(x()), p_inv(p_cos(x()))),
            p_tan(x()),
        ));

        // === Double angle formulas ===

        // sin(2x) = 2·sin(x)·cos(x) (expansion)
        rs.add(rule(
            "sin_double_angle",
            p_sin(p_mul(p_const(2.0), x())),
            p_mul(p_const(2.0), p_mul(p_sin(x()), p_cos(x()))),
        ));
        rs.add(rule(
            "sin_double_angle_rev",
            p_sin(p_mul(x(), p_const(2.0))),
            p_mul(p_const(2.0), p_mul(p_sin(x()), p_cos(x()))),
        ));

        // 2·sin(x)·cos(x) = sin(2x) (contraction - reduces complexity!)
        // Pattern: 2 * (sin(x) * cos(x)) or 2 * (cos(x) * sin(x))
        rs.add(rule(
            "double_angle_sin",
            p_mul(p_const(2.0), p_mul(p_sin(x()), p_cos(x()))),
            p_sin(p_mul(p_const(2.0), x())),
        ));
        rs.add(rule(
            "double_angle_sin_rev",
            p_mul(p_const(2.0), p_mul(p_cos(x()), p_sin(x()))),
            p_sin(p_mul(p_const(2.0), x())),
        ));

        // cos(2x) = cos²(x) - sin²(x) (expansion)
        rs.add(rule(
            "cos_double_angle",
            p_cos(p_mul(p_const(2.0), x())),
            p_add(
                p_pow(p_cos(x()), p_const(2.0)),
                p_neg(p_pow(p_sin(x()), p_const(2.0))),
            ),
        ));
        rs.add(rule(
            "cos_double_angle_rev",
            p_cos(p_mul(x(), p_const(2.0))),
            p_add(
                p_pow(p_cos(x()), p_const(2.0)),
                p_neg(p_pow(p_sin(x()), p_const(2.0))),
            ),
        ));

        // cos²(x) - sin²(x) = cos(2x) (contraction - reduces complexity!)
        rs.add(rule(
            "double_angle_cos",
            p_add(
                p_pow(p_cos(x()), p_const(2.0)),
                p_neg(p_pow(p_sin(x()), p_const(2.0))),
            ),
            p_cos(p_mul(p_const(2.0), x())),
        ));

        // === Power reduction formulas ===
        // These convert squared trig functions to double angle form.
        // Note: Expansion increases complexity; contraction reduces it.

        // sin²(x) = (1 - cos(2x))/2 (expansion - increases complexity)
        rs.add(rule(
            "sin_sq_to_double_angle",
            p_pow(p_sin(x()), p_const(2.0)),
            p_mul(
                p_inv(p_const(2.0)),
                p_add(p_const(1.0), p_neg(p_cos(p_mul(p_const(2.0), x())))),
            ),
        ));

        // (1 - cos(2x))/2 = sin²(x) (contraction - reduces complexity!)
        // Pattern: (1/2) * (1 + (-cos(2x)))
        rs.add(rule(
            "double_angle_to_sin_sq",
            p_mul(
                p_inv(p_const(2.0)),
                p_add(p_const(1.0), p_neg(p_cos(p_mul(p_const(2.0), x())))),
            ),
            p_pow(p_sin(x()), p_const(2.0)),
        ));
        // Also match (1 + (-cos(2x))) * (1/2)
        rs.add(rule(
            "double_angle_to_sin_sq_rev",
            p_mul(
                p_add(p_const(1.0), p_neg(p_cos(p_mul(p_const(2.0), x())))),
                p_inv(p_const(2.0)),
            ),
            p_pow(p_sin(x()), p_const(2.0)),
        ));

        // cos²(x) = (1 + cos(2x))/2 (expansion - increases complexity)
        rs.add(rule(
            "cos_sq_to_double_angle",
            p_pow(p_cos(x()), p_const(2.0)),
            p_mul(
                p_inv(p_const(2.0)),
                p_add(p_const(1.0), p_cos(p_mul(p_const(2.0), x()))),
            ),
        ));

        // (1 + cos(2x))/2 = cos²(x) (contraction - reduces complexity!)
        rs.add(rule(
            "double_angle_to_cos_sq",
            p_mul(
                p_inv(p_const(2.0)),
                p_add(p_const(1.0), p_cos(p_mul(p_const(2.0), x()))),
            ),
            p_pow(p_cos(x()), p_const(2.0)),
        ));
        // Also match (1 + cos(2x)) * (1/2)
        rs.add(rule(
            "double_angle_to_cos_sq_rev",
            p_mul(
                p_add(p_const(1.0), p_cos(p_mul(p_const(2.0), x()))),
                p_inv(p_const(2.0)),
            ),
            p_pow(p_cos(x()), p_const(2.0)),
        ));

        // === Sum/difference formulas ===
        // These use two independent wildcards (a and b).
        let a = || wildcard("a");
        let b = || wildcard("b");

        // sin(a + b) = sin(a)·cos(b) + cos(a)·sin(b)
        rs.add(rule(
            "sin_sum",
            p_sin(p_add(a(), b())),
            p_add(
                p_mul(p_sin(a()), p_cos(b())),
                p_mul(p_cos(a()), p_sin(b())),
            ),
        ));

        // sin(a)·cos(b) + cos(a)·sin(b) = sin(a + b) (contraction)
        rs.add(rule(
            "sin_sum_contract",
            p_add(
                p_mul(p_sin(a()), p_cos(b())),
                p_mul(p_cos(a()), p_sin(b())),
            ),
            p_sin(p_add(a(), b())),
        ));

        // sin(a - b) = sin(a)·cos(b) - cos(a)·sin(b)
        // a - b is represented as Add(a, Neg(b))
        rs.add(rule(
            "sin_diff",
            p_sin(p_add(a(), p_neg(b()))),
            p_add(
                p_mul(p_sin(a()), p_cos(b())),
                p_neg(p_mul(p_cos(a()), p_sin(b()))),
            ),
        ));

        // sin(a)·cos(b) - cos(a)·sin(b) = sin(a - b) (contraction)
        rs.add(rule(
            "sin_diff_contract",
            p_add(
                p_mul(p_sin(a()), p_cos(b())),
                p_neg(p_mul(p_cos(a()), p_sin(b()))),
            ),
            p_sin(p_add(a(), p_neg(b()))),
        ));

        // cos(a + b) = cos(a)·cos(b) - sin(a)·sin(b)
        rs.add(rule(
            "cos_sum",
            p_cos(p_add(a(), b())),
            p_add(
                p_mul(p_cos(a()), p_cos(b())),
                p_neg(p_mul(p_sin(a()), p_sin(b()))),
            ),
        ));

        // cos(a)·cos(b) - sin(a)·sin(b) = cos(a + b) (contraction)
        rs.add(rule(
            "cos_sum_contract",
            p_add(
                p_mul(p_cos(a()), p_cos(b())),
                p_neg(p_mul(p_sin(a()), p_sin(b()))),
            ),
            p_cos(p_add(a(), b())),
        ));

        // cos(a - b) = cos(a)·cos(b) + sin(a)·sin(b)
        rs.add(rule(
            "cos_diff",
            p_cos(p_add(a(), p_neg(b()))),
            p_add(
                p_mul(p_cos(a()), p_cos(b())),
                p_mul(p_sin(a()), p_sin(b())),
            ),
        ));

        // cos(a)·cos(b) + sin(a)·sin(b) = cos(a - b) (contraction)
        rs.add(rule(
            "cos_diff_contract",
            p_add(
                p_mul(p_cos(a()), p_cos(b())),
                p_mul(p_sin(a()), p_sin(b())),
            ),
            p_cos(p_add(a(), p_neg(b()))),
        ));

        // === Product-to-sum identities ===
        // These convert products of trig functions to sums/differences.

        // sin(a)·cos(b) = ½[sin(a+b) + sin(a-b)]
        rs.add(rule(
            "sin_cos_product_to_sum",
            p_mul(p_sin(a()), p_cos(b())),
            p_mul(
                p_inv(p_const(2.0)),
                p_add(
                    p_sin(p_add(a(), b())),
                    p_sin(p_add(a(), p_neg(b()))),
                ),
            ),
        ));

        // ½[sin(a+b) + sin(a-b)] = sin(a)·cos(b) (contraction - reduces complexity!)
        rs.add(rule(
            "sum_to_sin_cos_product",
            p_mul(
                p_inv(p_const(2.0)),
                p_add(
                    p_sin(p_add(a(), b())),
                    p_sin(p_add(a(), p_neg(b()))),
                ),
            ),
            p_mul(p_sin(a()), p_cos(b())),
        ));
        // Also match (sum) * (1/2)
        rs.add(rule(
            "sum_to_sin_cos_product_rev",
            p_mul(
                p_add(
                    p_sin(p_add(a(), b())),
                    p_sin(p_add(a(), p_neg(b()))),
                ),
                p_inv(p_const(2.0)),
            ),
            p_mul(p_sin(a()), p_cos(b())),
        ));

        // cos(a)·cos(b) = ½[cos(a+b) + cos(a-b)]
        rs.add(rule(
            "cos_cos_product_to_sum",
            p_mul(p_cos(a()), p_cos(b())),
            p_mul(
                p_inv(p_const(2.0)),
                p_add(
                    p_cos(p_add(a(), b())),
                    p_cos(p_add(a(), p_neg(b()))),
                ),
            ),
        ));

        // ½[cos(a+b) + cos(a-b)] = cos(a)·cos(b) (contraction)
        rs.add(rule(
            "sum_to_cos_cos_product",
            p_mul(
                p_inv(p_const(2.0)),
                p_add(
                    p_cos(p_add(a(), b())),
                    p_cos(p_add(a(), p_neg(b()))),
                ),
            ),
            p_mul(p_cos(a()), p_cos(b())),
        ));
        rs.add(rule(
            "sum_to_cos_cos_product_rev",
            p_mul(
                p_add(
                    p_cos(p_add(a(), b())),
                    p_cos(p_add(a(), p_neg(b()))),
                ),
                p_inv(p_const(2.0)),
            ),
            p_mul(p_cos(a()), p_cos(b())),
        ));

        // sin(a)·sin(b) = ½[cos(a-b) - cos(a+b)]
        rs.add(rule(
            "sin_sin_product_to_sum",
            p_mul(p_sin(a()), p_sin(b())),
            p_mul(
                p_inv(p_const(2.0)),
                p_add(
                    p_cos(p_add(a(), p_neg(b()))),
                    p_neg(p_cos(p_add(a(), b()))),
                ),
            ),
        ));

        // ½[cos(a-b) - cos(a+b)] = sin(a)·sin(b) (contraction)
        rs.add(rule(
            "sum_to_sin_sin_product",
            p_mul(
                p_inv(p_const(2.0)),
                p_add(
                    p_cos(p_add(a(), p_neg(b()))),
                    p_neg(p_cos(p_add(a(), b()))),
                ),
            ),
            p_mul(p_sin(a()), p_sin(b())),
        ));
        rs.add(rule(
            "sum_to_sin_sin_product_rev",
            p_mul(
                p_add(
                    p_cos(p_add(a(), p_neg(b()))),
                    p_neg(p_cos(p_add(a(), b()))),
                ),
                p_inv(p_const(2.0)),
            ),
            p_mul(p_sin(a()), p_sin(b())),
        ));

        // === Inverse trig compositions (principal value) ===
        rs.add(rule("asin_sin", p_asin(p_sin(x())), x()));
        rs.add(rule("acos_cos", p_acos(p_cos(x())), x()));
        rs.add(rule("atan_tan", p_atan(p_tan(x())), x()));

        // === Exponential/Log identities ===
        rs.add(rule("exp_zero", p_exp(p_const(0.0)), p_const(1.0))); // e^0 = 1
        rs.add(rule("ln_one", p_ln(p_const(1.0)), p_const(0.0))); // ln(1) = 0
        rs.add(rule("ln_e", p_ln(p_const(E)), p_const(1.0))); // ln(e) = 1
        rs.add(rule("exp_ln", p_exp(p_ln(x())), x())); // e^(ln(x)) = x
        rs.add(rule("ln_exp", p_ln(p_exp(x())), x())); // ln(e^x) = x

        // === Logarithm/Exponential extensions ===

        // ln(a·b) = ln(a) + ln(b) (expansion)
        rs.add(rule(
            "ln_product",
            p_ln(p_mul(a(), b())),
            p_add(p_ln(a()), p_ln(b())),
        ));

        // ln(a) + ln(b) = ln(a·b) (contraction - reduces complexity!)
        rs.add(rule(
            "ln_product_contract",
            p_add(p_ln(a()), p_ln(b())),
            p_ln(p_mul(a(), b())),
        ));

        // ln(a/b) = ln(a) - ln(b), where a/b is represented as a * (1/b)
        rs.add(rule(
            "ln_quotient",
            p_ln(p_mul(a(), p_inv(b()))),
            p_add(p_ln(a()), p_neg(p_ln(b()))),
        ));

        // ln(a) - ln(b) = ln(a/b) (contraction)
        rs.add(rule(
            "ln_quotient_contract",
            p_add(p_ln(a()), p_neg(p_ln(b()))),
            p_ln(p_mul(a(), p_inv(b()))),
        ));

        // ln(a^n) = n·ln(a) (expansion)
        rs.add(rule(
            "ln_power",
            p_ln(p_pow(a(), b())),
            p_mul(b(), p_ln(a())),
        ));

        // n·ln(a) = ln(a^n) (contraction - reduces complexity!)
        rs.add(rule(
            "ln_power_contract",
            p_mul(b(), p_ln(a())),
            p_ln(p_pow(a(), b())),
        ));
        // Also match ln(a) * n
        rs.add(rule(
            "ln_power_contract_rev",
            p_mul(p_ln(a()), b()),
            p_ln(p_pow(a(), b())),
        ));

        // exp(a + b) = exp(a)·exp(b) (expansion)
        rs.add(rule(
            "exp_sum",
            p_exp(p_add(a(), b())),
            p_mul(p_exp(a()), p_exp(b())),
        ));

        // exp(a)·exp(b) = exp(a + b) (contraction - reduces complexity!)
        rs.add(rule(
            "exp_sum_contract",
            p_mul(p_exp(a()), p_exp(b())),
            p_exp(p_add(a(), b())),
        ));

        // exp(a - b) = exp(a)/exp(b) = exp(a) * (1/exp(b))
        rs.add(rule(
            "exp_diff",
            p_exp(p_add(a(), p_neg(b()))),
            p_mul(p_exp(a()), p_inv(p_exp(b()))),
        ));

        // exp(a) * (1/exp(b)) = exp(a - b) (contraction)
        rs.add(rule(
            "exp_diff_contract",
            p_mul(p_exp(a()), p_inv(p_exp(b()))),
            p_exp(p_add(a(), p_neg(b()))),
        ));

        rs
    }

    /// Tensor algebra rules.
    ///
    /// Includes index-aware rules using Pattern::Var for tensor operations:
    /// - Kronecker delta contractions: δ^i_j v^j = v^i
    /// - Metric tensor index lowering: g_ij v^j = v_i
    /// - Metric tensor index raising: g^ij v_j = v^i
    /// - Metric tensor identity: g^ik g_kj = δ^i_j
    /// - Metric tensor symmetry: g_ij = g_ji, g^ij = g^ji
    /// - Levi-Civita antisymmetry: ε_ij = -ε_ji, ε^ij = -ε^ji
    /// - Electromagnetic field tensor antisymmetry: F^μν = -F^νμ
    ///
    /// For custom tensors, use the helper methods:
    /// - `add_symmetric(name)`: T_ij = T_ji and T^ij = T^ji
    /// - `add_antisymmetric(name)`: A_ij = -A_ji and A^ij = -A^ji
    /// - `add_symmetric_lower/upper(name)`: single index position only
    /// - `add_antisymmetric_lower/upper(name)`: single index position only
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
        rs.add(rule(
            "neg_mul",
            p_neg(p_mul(x(), y())),
            p_mul(p_neg(x()), y()),
        ));

        // === Kronecker delta contraction ===
        // δ^i_j * v^j = v^i (delta contracts with vector, result has free index)
        rs.add(rule(
            "kronecker_delta_right",
            p_mul(
                p_var("δ", vec![idx_upper("i"), idx_lower("j")]),
                p_var_wild("v", vec![idx_upper("j")]),
            ),
            p_var_wild("v", vec![idx_upper("i")]),
        ));

        // v^j * δ^i_j = v^i (commuted form)
        rs.add(rule(
            "kronecker_delta_left",
            p_mul(
                p_var_wild("v", vec![idx_upper("j")]),
                p_var("δ", vec![idx_upper("i"), idx_lower("j")]),
            ),
            p_var_wild("v", vec![idx_upper("i")]),
        ));

        // δ^i_j * w_j = w_i (contraction with covector)
        rs.add(rule(
            "kronecker_delta_covector_right",
            p_mul(
                p_var("δ", vec![idx_upper("i"), idx_lower("j")]),
                p_var_wild("w", vec![idx_lower("j")]),
            ),
            p_var_wild("w", vec![idx_lower("i")]),
        ));

        // w_j * δ^i_j = w_i (commuted form with covector)
        rs.add(rule(
            "kronecker_delta_covector_left",
            p_mul(
                p_var_wild("w", vec![idx_lower("j")]),
                p_var("δ", vec![idx_upper("i"), idx_lower("j")]),
            ),
            p_var_wild("w", vec![idx_lower("i")]),
        ));

        // === Metric tensor contraction ===
        // The metric tensor g_ij lowers indices, g^ij raises indices.

        // Index lowering: g_ij * v^j = v_i
        rs.add(rule(
            "metric_lower_right",
            p_mul(
                p_var("g", vec![idx_lower("i"), idx_lower("j")]),
                p_var_wild("v", vec![idx_upper("j")]),
            ),
            p_var_wild("v", vec![idx_lower("i")]),
        ));

        // v^j * g_ij = v_i (commuted form)
        rs.add(rule(
            "metric_lower_left",
            p_mul(
                p_var_wild("v", vec![idx_upper("j")]),
                p_var("g", vec![idx_lower("i"), idx_lower("j")]),
            ),
            p_var_wild("v", vec![idx_lower("i")]),
        ));

        // Index raising: g^ij * v_j = v^i
        rs.add(rule(
            "metric_raise_right",
            p_mul(
                p_var("g", vec![idx_upper("i"), idx_upper("j")]),
                p_var_wild("v", vec![idx_lower("j")]),
            ),
            p_var_wild("v", vec![idx_upper("i")]),
        ));

        // v_j * g^ij = v^i (commuted form)
        rs.add(rule(
            "metric_raise_left",
            p_mul(
                p_var_wild("v", vec![idx_lower("j")]),
                p_var("g", vec![idx_upper("i"), idx_upper("j")]),
            ),
            p_var_wild("v", vec![idx_upper("i")]),
        ));

        // Metric tensor identity: g^ik * g_kj = δ^i_j
        rs.add(rule(
            "metric_inverse_right",
            p_mul(
                p_var("g", vec![idx_upper("i"), idx_upper("k")]),
                p_var("g", vec![idx_lower("k"), idx_lower("j")]),
            ),
            p_var("δ", vec![idx_upper("i"), idx_lower("j")]),
        ));

        // g_kj * g^ik = δ^i_j (commuted form)
        rs.add(rule(
            "metric_inverse_left",
            p_mul(
                p_var("g", vec![idx_lower("k"), idx_lower("j")]),
                p_var("g", vec![idx_upper("i"), idx_upper("k")]),
            ),
            p_var("δ", vec![idx_upper("i"), idx_lower("j")]),
        ));

        // === Tensor symmetry ===
        // The metric tensor is symmetric: g_ij = g_ji, g^ij = g^ji
        // These rules allow beam search to explore equivalent index orderings.

        // g_ij = g_ji (covariant metric symmetry)
        rs.add(rule(
            "metric_symmetric_lower",
            p_var("g", vec![idx_lower("i"), idx_lower("j")]),
            p_var("g", vec![idx_lower("j"), idx_lower("i")]),
        ));

        // g^ij = g^ji (contravariant metric symmetry)
        rs.add(rule(
            "metric_symmetric_upper",
            p_var("g", vec![idx_upper("i"), idx_upper("j")]),
            p_var("g", vec![idx_upper("j"), idx_upper("i")]),
        ));

        // The Kronecker delta is also symmetric in a generalized sense:
        // δ^i_j with indices swapped yields the same contraction behavior
        // (though typically written with upper first, lower second)

        // === Antisymmetric tensors ===
        // The Levi-Civita symbol ε is totally antisymmetric.
        // For rank-2: ε_ij = -ε_ji, ε^ij = -ε^ji

        // ε_ij = -ε_ji (covariant Levi-Civita antisymmetry)
        rs.add(rule(
            "levi_civita_antisymmetric_lower",
            p_var("ε", vec![idx_lower("i"), idx_lower("j")]),
            p_neg(p_var("ε", vec![idx_lower("j"), idx_lower("i")])),
        ));

        // ε^ij = -ε^ji (contravariant Levi-Civita antisymmetry)
        rs.add(rule(
            "levi_civita_antisymmetric_upper",
            p_var("ε", vec![idx_upper("i"), idx_upper("j")]),
            p_neg(p_var("ε", vec![idx_upper("j"), idx_upper("i")])),
        ));

        // The electromagnetic field tensor F is also antisymmetric: F^μν = -F^νμ
        rs.add(rule(
            "em_field_antisymmetric_upper",
            p_var("F", vec![idx_upper("i"), idx_upper("j")]),
            p_neg(p_var("F", vec![idx_upper("j"), idx_upper("i")])),
        ));

        rs.add(rule(
            "em_field_antisymmetric_lower",
            p_var("F", vec![idx_lower("i"), idx_lower("j")]),
            p_neg(p_var("F", vec![idx_lower("j"), idx_lower("i")])),
        ));

        rs
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

    /// Add symmetry rules for a rank-2 tensor with two lower indices.
    /// Generates: T_ij = T_ji
    ///
    /// # Example
    /// ```
    /// use erd_symbolic::RuleSet;
    /// let mut rules = RuleSet::new();
    /// rules.add_symmetric_lower("h"); // h_ij = h_ji (e.g., perturbation metric)
    /// ```
    pub fn add_symmetric_lower(&mut self, tensor_name: &str) -> &mut Self {
        self.add(rule(
            &format!("{}_symmetric_lower", tensor_name),
            p_var(tensor_name, vec![idx_lower("i"), idx_lower("j")]),
            p_var(tensor_name, vec![idx_lower("j"), idx_lower("i")]),
        ))
    }

    /// Add symmetry rules for a rank-2 tensor with two upper indices.
    /// Generates: T^ij = T^ji
    ///
    /// # Example
    /// ```
    /// use erd_symbolic::RuleSet;
    /// let mut rules = RuleSet::new();
    /// rules.add_symmetric_upper("h"); // h^ij = h^ji
    /// ```
    pub fn add_symmetric_upper(&mut self, tensor_name: &str) -> &mut Self {
        self.add(rule(
            &format!("{}_symmetric_upper", tensor_name),
            p_var(tensor_name, vec![idx_upper("i"), idx_upper("j")]),
            p_var(tensor_name, vec![idx_upper("j"), idx_upper("i")]),
        ))
    }

    /// Add symmetry rules for a rank-2 tensor (both index positions).
    /// Generates: T_ij = T_ji and T^ij = T^ji
    ///
    /// # Example
    /// ```
    /// use erd_symbolic::RuleSet;
    /// let mut rules = RuleSet::new();
    /// rules.add_symmetric("S"); // S_ij = S_ji, S^ij = S^ji
    /// ```
    pub fn add_symmetric(&mut self, tensor_name: &str) -> &mut Self {
        self.add_symmetric_lower(tensor_name)
            .add_symmetric_upper(tensor_name)
    }

    /// Add antisymmetry rules for a rank-2 tensor with two lower indices.
    /// Generates: A_ij = -A_ji
    ///
    /// # Example
    /// ```
    /// use erd_symbolic::RuleSet;
    /// let mut rules = RuleSet::new();
    /// rules.add_antisymmetric_lower("ω"); // ω_ij = -ω_ji (e.g., vorticity tensor)
    /// ```
    pub fn add_antisymmetric_lower(&mut self, tensor_name: &str) -> &mut Self {
        self.add(rule(
            &format!("{}_antisymmetric_lower", tensor_name),
            p_var(tensor_name, vec![idx_lower("i"), idx_lower("j")]),
            p_neg(p_var(tensor_name, vec![idx_lower("j"), idx_lower("i")])),
        ))
    }

    /// Add antisymmetry rules for a rank-2 tensor with two upper indices.
    /// Generates: A^ij = -A^ji
    ///
    /// # Example
    /// ```
    /// use erd_symbolic::RuleSet;
    /// let mut rules = RuleSet::new();
    /// rules.add_antisymmetric_upper("ω"); // ω^ij = -ω^ji
    /// ```
    pub fn add_antisymmetric_upper(&mut self, tensor_name: &str) -> &mut Self {
        self.add(rule(
            &format!("{}_antisymmetric_upper", tensor_name),
            p_var(tensor_name, vec![idx_upper("i"), idx_upper("j")]),
            p_neg(p_var(tensor_name, vec![idx_upper("j"), idx_upper("i")])),
        ))
    }

    /// Add antisymmetry rules for a rank-2 tensor (both index positions).
    /// Generates: A_ij = -A_ji and A^ij = -A^ji
    ///
    /// # Example
    /// ```
    /// use erd_symbolic::RuleSet;
    /// let mut rules = RuleSet::new();
    /// rules.add_antisymmetric("A"); // A_ij = -A_ji, A^ij = -A^ji
    /// ```
    pub fn add_antisymmetric(&mut self, tensor_name: &str) -> &mut Self {
        self.add_antisymmetric_lower(tensor_name)
            .add_antisymmetric_upper(tensor_name)
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
    use crate::expr::{
        acos, add, asin, atan, clamp, constant, cos, cosh, exp, floor, inv, ln, max, min, mul,
        neg, round, scalar, sign, sin, sinh, tan, tanh,
    };
    use crate::pow;

    #[test]
    fn match_wildcard() {
        let pattern = wildcard("x");
        let expr = scalar("a");
        let bindings = pattern.match_expr(&expr).unwrap();
        assert_eq!(bindings.exprs.get("x"), Some(&expr));
    }

    #[test]
    fn match_const_wild_succeeds() {
        let pattern = const_wild("c");
        let expr = constant(42.0);
        let bindings = pattern.match_expr(&expr).unwrap();
        assert_eq!(bindings.exprs.get("c"), Some(&expr));
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
        assert_eq!(bindings.exprs.get("x"), Some(&scalar("a")));
        assert_eq!(bindings.exprs.get("y"), Some(&scalar("b")));
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
        assert_eq!(bindings.exprs.get("x"), Some(&scalar("a")));
        assert_eq!(bindings.exprs.get("y"), Some(&scalar("b")));
        assert_eq!(bindings.exprs.get("z"), Some(&scalar("c")));
    }

    #[test]
    fn match_repeated_wildcard_same_expr() {
        // Pattern: x + x (same wildcard used twice)
        let pattern = p_add(wildcard("x"), wildcard("x"));
        // Expression: a + a (same sub-expression)
        let expr = add(scalar("a"), scalar("a"));
        let bindings = pattern.match_expr(&expr).unwrap();
        assert_eq!(bindings.exprs.get("x"), Some(&scalar("a")));
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
        assert_eq!(bindings.exprs.get("x"), Some(&scalar("a")));
    }

    #[test]
    fn match_complex_algebraic_identity() {
        // Pattern for x^2: x * x
        let pattern = p_mul(wildcard("x"), wildcard("x"));
        // Expression: a * a
        let expr = mul(scalar("a"), scalar("a"));
        let bindings = pattern.match_expr(&expr).unwrap();
        assert_eq!(bindings.exprs.get("x"), Some(&scalar("a")));
    }

    // === substitute tests ===

    #[test]
    fn substitute_wildcard() {
        let pattern = wildcard("x");
        let mut bindings = Bindings::new();
        bindings.exprs.insert("x".to_string(), scalar("a"));
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
        bindings.exprs.insert("x".to_string(), scalar("a"));
        bindings.exprs.insert("y".to_string(), scalar("b"));
        assert_eq!(pattern.substitute(&bindings), add(scalar("a"), scalar("b")));
    }

    #[test]
    fn substitute_nested() {
        // Pattern: (x + y) * z
        let pattern = p_mul(p_add(wildcard("x"), wildcard("y")), wildcard("z"));
        let mut bindings = Bindings::new();
        bindings.exprs.insert("x".to_string(), constant(1.0));
        bindings.exprs.insert("y".to_string(), constant(2.0));
        bindings.exprs.insert("z".to_string(), scalar("a"));
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
        bindings.exprs.insert("x".to_string(), scalar("a"));
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

    #[test]
    fn standard_min_idempotent() {
        let rs = RuleSet::standard();
        let expr = min(scalar("a"), scalar("a"));

        let result = rs
            .iter()
            .find(|r| r.name == "min_idempotent")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(scalar("a")));
    }

    #[test]
    fn standard_max_idempotent() {
        let rs = RuleSet::standard();
        let expr = max(scalar("a"), scalar("a"));

        let result = rs
            .iter()
            .find(|r| r.name == "max_idempotent")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(scalar("a")));
    }

    #[test]
    fn standard_clamp_idempotent() {
        let rs = RuleSet::standard();
        let expr = clamp(
            clamp(scalar("x"), constant(0.0), constant(1.0)),
            constant(0.0),
            constant(1.0),
        );

        let result = rs
            .iter()
            .find(|r| r.name == "clamp_idempotent")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(
            result,
            Some(clamp(scalar("x"), constant(0.0), constant(1.0)))
        );
    }

    // === Extended RuleSet tests ===

    #[test]
    fn extended_min_commute() {
        let rs = RuleSet::extended();
        let expr = min(scalar("a"), scalar("b"));

        let result = rs
            .iter()
            .find(|r| r.name == "min_commute")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(min(scalar("b"), scalar("a"))));
    }

    #[test]
    fn extended_max_commute() {
        let rs = RuleSet::extended();
        let expr = max(scalar("a"), scalar("b"));

        let result = rs
            .iter()
            .find(|r| r.name == "max_commute")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(max(scalar("b"), scalar("a"))));
    }

    #[test]
    fn extended_min_absorb() {
        let rs = RuleSet::extended();
        let expr = min(scalar("a"), max(scalar("a"), scalar("b")));

        let result = rs
            .iter()
            .find(|r| r.name == "min_absorb")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(scalar("a")));
    }

    #[test]
    fn extended_max_absorb() {
        let rs = RuleSet::extended();
        let expr = max(scalar("a"), min(scalar("a"), scalar("b")));

        let result = rs
            .iter()
            .find(|r| r.name == "max_absorb")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(scalar("a")));
    }

    #[test]
    fn extended_min_max_sum() {
        let rs = RuleSet::extended();
        let expr = add(min(scalar("a"), scalar("b")), max(scalar("a"), scalar("b")));

        let result = rs
            .iter()
            .find(|r| r.name == "min_max_sum")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(add(scalar("a"), scalar("b"))));
    }

    #[test]
    fn extended_clamp_def() {
        let rs = RuleSet::extended();
        let expr = clamp(scalar("x"), scalar("lo"), scalar("hi"));

        let result = rs
            .iter()
            .find(|r| r.name == "clamp_def")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(
            result,
            Some(min(max(scalar("x"), scalar("lo")), scalar("hi")))
        );
    }

    #[test]
    fn extended_sign_neg() {
        let rs = RuleSet::extended();
        let expr = sign(neg(scalar("a")));

        let result = rs
            .iter()
            .find(|r| r.name == "sign_neg")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(neg(sign(scalar("a")))));
    }

    #[test]
    fn extended_sign_square() {
        let rs = RuleSet::extended();
        let expr = pow(sign(scalar("a")), constant(2.0));

        let result = rs
            .iter()
            .find(|r| r.name == "sign_square")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(constant(1.0)));
    }

    #[test]
    fn extended_round_def() {
        let rs = RuleSet::extended();
        let expr = round(scalar("a"));

        let result = rs
            .iter()
            .find(|r| r.name == "round_def")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(
            result,
            Some(floor(add(scalar("a"), inv(constant(2.0)))))
        );
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
    fn trig_tan_zero() {
        let rs = RuleSet::trigonometric();
        let expr = tan(constant(0.0));

        let result = rs
            .iter()
            .find(|r| r.name == "tan_zero")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(constant(0.0)));
    }

    #[test]
    fn trig_tan_neg() {
        let rs = RuleSet::trigonometric();
        let expr = tan(neg(scalar("a")));

        let result = rs
            .iter()
            .find(|r| r.name == "tan_neg")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(neg(tan(scalar("a")))));
    }

    #[test]
    fn trig_tan_period() {
        let rs = RuleSet::trigonometric();
        let expr = tan(add(scalar("a"), constant(std::f64::consts::PI)));

        let result = rs
            .iter()
            .find(|r| r.name == "tan_period")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(tan(scalar("a"))));
    }

    #[test]
    fn trig_tan_def() {
        let rs = RuleSet::trigonometric();
        let expr = tan(scalar("a"));

        let result = rs
            .iter()
            .find(|r| r.name == "tan_def")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(
            result,
            Some(mul(sin(scalar("a")), inv(cos(scalar("a")))))
        );
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
    fn trig_sinh_zero() {
        let rs = RuleSet::trigonometric();
        let expr = sinh(constant(0.0));

        let result = rs
            .iter()
            .find(|r| r.name == "sinh_zero")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(constant(0.0)));
    }

    #[test]
    fn trig_cosh_zero() {
        let rs = RuleSet::trigonometric();
        let expr = cosh(constant(0.0));

        let result = rs
            .iter()
            .find(|r| r.name == "cosh_zero")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(constant(1.0)));
    }

    #[test]
    fn trig_tanh_zero() {
        let rs = RuleSet::trigonometric();
        let expr = tanh(constant(0.0));

        let result = rs
            .iter()
            .find(|r| r.name == "tanh_zero")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(constant(0.0)));
    }

    #[test]
    fn trig_sinh_neg() {
        let rs = RuleSet::trigonometric();
        let expr = sinh(neg(scalar("a")));

        let result = rs
            .iter()
            .find(|r| r.name == "sinh_neg")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(neg(sinh(scalar("a")))));
    }

    #[test]
    fn trig_cosh_neg() {
        let rs = RuleSet::trigonometric();
        let expr = cosh(neg(scalar("a")));

        let result = rs
            .iter()
            .find(|r| r.name == "cosh_neg")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(cosh(scalar("a"))));
    }

    #[test]
    fn trig_tanh_neg() {
        let rs = RuleSet::trigonometric();
        let expr = tanh(neg(scalar("a")));

        let result = rs
            .iter()
            .find(|r| r.name == "tanh_neg")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(neg(tanh(scalar("a")))));
    }

    #[test]
    fn trig_hyperbolic_identity() {
        let rs = RuleSet::trigonometric();
        let expr = add(
            pow(cosh(scalar("a")), constant(2.0)),
            neg(pow(sinh(scalar("a")), constant(2.0))),
        );

        let result = rs
            .iter()
            .find(|r| r.name == "hyperbolic_identity")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(constant(1.0)));
    }

    #[test]
    fn trig_sinh_double_angle() {
        let rs = RuleSet::trigonometric();
        let expr = sinh(mul(constant(2.0), scalar("a")));

        let result = rs
            .iter()
            .find(|r| r.name == "sinh_double_angle")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(
            result,
            Some(mul(
                constant(2.0),
                mul(sinh(scalar("a")), cosh(scalar("a")))
            ))
        );
    }

    #[test]
    fn trig_cosh_double_angle() {
        let rs = RuleSet::trigonometric();
        let expr = cosh(mul(constant(2.0), scalar("a")));

        let result = rs
            .iter()
            .find(|r| r.name == "cosh_double_angle")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(
            result,
            Some(add(
                pow(cosh(scalar("a")), constant(2.0)),
                pow(sinh(scalar("a")), constant(2.0)),
            ))
        );
    }

    #[test]
    fn trig_tanh_def() {
        let rs = RuleSet::trigonometric();
        let expr = tanh(scalar("a"));

        let result = rs
            .iter()
            .find(|r| r.name == "tanh_def")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(
            result,
            Some(mul(sinh(scalar("a")), inv(cosh(scalar("a")))))
        );
    }

    #[test]
    fn trig_asin_sin() {
        let rs = RuleSet::trigonometric();
        let expr = asin(sin(scalar("a")));

        let result = rs
            .iter()
            .find(|r| r.name == "asin_sin")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(scalar("a")));
    }

    #[test]
    fn trig_acos_cos() {
        let rs = RuleSet::trigonometric();
        let expr = acos(cos(scalar("a")));

        let result = rs
            .iter()
            .find(|r| r.name == "acos_cos")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(scalar("a")));
    }

    #[test]
    fn trig_atan_tan() {
        let rs = RuleSet::trigonometric();
        let expr = atan(tan(scalar("a")));

        let result = rs
            .iter()
            .find(|r| r.name == "atan_tan")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(scalar("a")));
    }

    #[test]
    fn trig_acos_zero() {
        let rs = RuleSet::trigonometric();
        let expr = acos(constant(0.0));

        let result = rs
            .iter()
            .find(|r| r.name == "acos_zero")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(constant(std::f64::consts::FRAC_PI_2)));
    }

    #[test]
    fn trig_atan_one() {
        let rs = RuleSet::trigonometric();
        let expr = atan(constant(1.0));

        let result = rs
            .iter()
            .find(|r| r.name == "atan_one")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(constant(std::f64::consts::FRAC_PI_4)));
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

    // === Index-aware pattern matching tests ===

    use crate::expr::{lower, tensor, upper};

    #[test]
    fn match_var_exact_name_no_indices() {
        // Pattern: match variable named "x" with no indices
        let pattern = p_var("x", vec![]);
        let expr = scalar("x");
        let bindings = pattern.match_expr(&expr).unwrap();
        assert!(bindings.exprs.is_empty());
        assert!(bindings.indices.is_empty());
    }

    #[test]
    fn match_var_exact_name_fails_wrong_name() {
        let pattern = p_var("x", vec![]);
        let expr = scalar("y");
        assert!(pattern.match_expr(&expr).is_none());
    }

    #[test]
    fn match_var_exact_name_with_upper_index() {
        // Pattern: match "v" with one upper index, bind index name to "i"
        let pattern = p_var("v", vec![idx_upper("i")]);
        let expr = tensor("v", vec![upper("mu")]);
        let bindings = pattern.match_expr(&expr).unwrap();
        assert_eq!(bindings.indices.get("i"), Some(&"mu".to_string()));
    }

    #[test]
    fn match_var_exact_name_with_lower_index() {
        // Pattern: match "w" with one lower index, bind to "j"
        let pattern = p_var("w", vec![idx_lower("j")]);
        let expr = tensor("w", vec![lower("nu")]);
        let bindings = pattern.match_expr(&expr).unwrap();
        assert_eq!(bindings.indices.get("j"), Some(&"nu".to_string()));
    }

    #[test]
    fn match_var_fails_wrong_index_position() {
        // Pattern expects upper, expr has lower
        let pattern = p_var("v", vec![idx_upper("i")]);
        let expr = tensor("v", vec![lower("mu")]);
        assert!(pattern.match_expr(&expr).is_none());
    }

    #[test]
    fn match_var_fails_wrong_index_count() {
        // Pattern expects 1 index, expr has 2
        let pattern = p_var("T", vec![idx_upper("i")]);
        let expr = tensor("T", vec![upper("i"), lower("j")]);
        assert!(pattern.match_expr(&expr).is_none());
    }

    #[test]
    fn match_var_mixed_indices() {
        // Pattern: T^i_j (one upper, one lower)
        let pattern = p_var("T", vec![idx_upper("i"), idx_lower("j")]);
        let expr = tensor("T", vec![upper("mu"), lower("nu")]);
        let bindings = pattern.match_expr(&expr).unwrap();
        assert_eq!(bindings.indices.get("i"), Some(&"mu".to_string()));
        assert_eq!(bindings.indices.get("j"), Some(&"nu".to_string()));
    }

    #[test]
    fn match_var_wild_name() {
        // Pattern: any variable name (bind to "var"), with one upper index
        let pattern = p_var_wild("var", vec![idx_upper("i")]);
        let expr = tensor("velocity", vec![upper("x")]);
        let bindings = pattern.match_expr(&expr).unwrap();
        // The variable itself is bound
        assert_eq!(bindings.exprs.get("var"), Some(&expr));
        assert_eq!(bindings.indices.get("i"), Some(&"x".to_string()));
    }

    #[test]
    fn match_var_repeated_index_wildcard_same() {
        // Pattern: T^i_i (same index wildcard for both positions)
        // This should match when both indices have the same name
        let pattern = p_var("T", vec![idx_upper("i"), idx_lower("i")]);
        let expr = tensor("T", vec![upper("mu"), lower("mu")]);
        let bindings = pattern.match_expr(&expr).unwrap();
        assert_eq!(bindings.indices.get("i"), Some(&"mu".to_string()));
    }

    #[test]
    fn match_var_repeated_index_wildcard_different_fails() {
        // Pattern: T^i_i (same wildcard) but expr has different index names
        let pattern = p_var("T", vec![idx_upper("i"), idx_lower("i")]);
        let expr = tensor("T", vec![upper("mu"), lower("nu")]);
        assert!(pattern.match_expr(&expr).is_none());
    }

    #[test]
    fn substitute_var_exact() {
        // Substitute a Var pattern with exact name and index wildcards
        let pattern = p_var("v", vec![idx_upper("i")]);
        let mut bindings = Bindings::new();
        bindings
            .indices
            .insert("i".to_string(), "alpha".to_string());

        let result = pattern.substitute(&bindings);
        assert_eq!(result, tensor("v", vec![upper("alpha")]));
    }

    #[test]
    fn substitute_var_with_bound_indices() {
        // More complex: T^i_j with bound index wildcards
        let pattern = p_var("T", vec![idx_upper("i"), idx_lower("j")]);
        let mut bindings = Bindings::new();
        bindings.indices.insert("i".to_string(), "mu".to_string());
        bindings.indices.insert("j".to_string(), "nu".to_string());

        let result = pattern.substitute(&bindings);
        assert_eq!(result, tensor("T", vec![upper("mu"), lower("nu")]));
    }

    #[test]
    fn var_pattern_match_then_substitute_roundtrip() {
        // Match a tensor, then substitute back
        let pattern = p_var("A", vec![idx_upper("i"), idx_lower("j")]);
        let expr = tensor("A", vec![upper("mu"), lower("nu")]);

        let bindings = pattern.match_expr(&expr).unwrap();
        let result = pattern.substitute(&bindings);
        assert_eq!(result, expr);
    }

    #[test]
    fn kronecker_delta_contraction_pattern() {
        // This tests the building blocks for δ^i_j * v^j = v^i
        // Pattern: δ^i_j (Kronecker delta with upper i, lower j)
        let delta_pattern = p_var("δ", vec![idx_upper("i"), idx_lower("j")]);
        let delta_expr = tensor("δ", vec![upper("mu"), lower("nu")]);

        let bindings = delta_pattern.match_expr(&delta_expr).unwrap();
        assert_eq!(bindings.indices.get("i"), Some(&"mu".to_string()));
        assert_eq!(bindings.indices.get("j"), Some(&"nu".to_string()));

        // Now test that we can construct a result using the bound indices
        // v^i pattern with the bound "i" should give v^mu
        let result_pattern = p_var("v", vec![idx_upper("i")]);
        let result = result_pattern.substitute(&bindings);
        assert_eq!(result, tensor("v", vec![upper("mu")]));
    }

    // === Kronecker delta rule tests ===

    #[test]
    fn kronecker_delta_rule_vector_right() {
        // δ^μ_ν * A^ν = A^μ
        let rs = RuleSet::tensor();
        let expr = mul(
            tensor("δ", vec![upper("mu"), lower("nu")]),
            tensor("A", vec![upper("nu")]),
        );

        let result = rs
            .iter()
            .find(|r| r.name == "kronecker_delta_right")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(tensor("A", vec![upper("mu")])));
    }

    #[test]
    fn kronecker_delta_rule_vector_left() {
        // A^ν * δ^μ_ν = A^μ
        let rs = RuleSet::tensor();
        let expr = mul(
            tensor("A", vec![upper("nu")]),
            tensor("δ", vec![upper("mu"), lower("nu")]),
        );

        let result = rs
            .iter()
            .find(|r| r.name == "kronecker_delta_left")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(tensor("A", vec![upper("mu")])));
    }

    #[test]
    fn kronecker_delta_rule_covector_right() {
        // δ^μ_ν * B_ν = B_μ (note: lower index on result follows the pattern)
        let rs = RuleSet::tensor();
        let expr = mul(
            tensor("δ", vec![upper("mu"), lower("nu")]),
            tensor("B", vec![lower("nu")]),
        );

        let result = rs
            .iter()
            .find(|r| r.name == "kronecker_delta_covector_right")
            .and_then(|r| r.apply_ltr(&expr));

        // The covector rule maps δ^i_j * w_j -> w_i (lower index result)
        assert_eq!(result, Some(tensor("B", vec![lower("mu")])));
    }

    #[test]
    fn kronecker_delta_rule_covector_left() {
        // B_ν * δ^μ_ν = B_μ
        let rs = RuleSet::tensor();
        let expr = mul(
            tensor("B", vec![lower("nu")]),
            tensor("δ", vec![upper("mu"), lower("nu")]),
        );

        let result = rs
            .iter()
            .find(|r| r.name == "kronecker_delta_covector_left")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(tensor("B", vec![lower("mu")])));
    }

    #[test]
    fn kronecker_delta_no_match_different_indices() {
        // δ^μ_ν * A^σ should NOT match (indices don't contract)
        let rs = RuleSet::tensor();
        let expr = mul(
            tensor("δ", vec![upper("mu"), lower("nu")]),
            tensor("A", vec![upper("sigma")]), // Different index
        );

        let result = rs
            .iter()
            .find(|r| r.name == "kronecker_delta_right")
            .and_then(|r| r.apply_ltr(&expr));

        assert!(result.is_none());
    }

    // === Metric tensor rule tests ===

    #[test]
    fn metric_lower_right() {
        // g_μν * v^ν = v_μ
        let rs = RuleSet::tensor();
        let expr = mul(
            tensor("g", vec![lower("mu"), lower("nu")]),
            tensor("v", vec![upper("nu")]),
        );

        let result = rs
            .iter()
            .find(|r| r.name == "metric_lower_right")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(tensor("v", vec![lower("mu")])));
    }

    #[test]
    fn metric_lower_left() {
        // v^ν * g_μν = v_μ
        let rs = RuleSet::tensor();
        let expr = mul(
            tensor("v", vec![upper("nu")]),
            tensor("g", vec![lower("mu"), lower("nu")]),
        );

        let result = rs
            .iter()
            .find(|r| r.name == "metric_lower_left")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(tensor("v", vec![lower("mu")])));
    }

    #[test]
    fn metric_raise_right() {
        // g^μν * v_ν = v^μ
        let rs = RuleSet::tensor();
        let expr = mul(
            tensor("g", vec![upper("mu"), upper("nu")]),
            tensor("v", vec![lower("nu")]),
        );

        let result = rs
            .iter()
            .find(|r| r.name == "metric_raise_right")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(tensor("v", vec![upper("mu")])));
    }

    #[test]
    fn metric_raise_left() {
        // v_ν * g^μν = v^μ
        let rs = RuleSet::tensor();
        let expr = mul(
            tensor("v", vec![lower("nu")]),
            tensor("g", vec![upper("mu"), upper("nu")]),
        );

        let result = rs
            .iter()
            .find(|r| r.name == "metric_raise_left")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(tensor("v", vec![upper("mu")])));
    }

    #[test]
    fn metric_inverse_right() {
        // g^μκ * g_κν = δ^μ_ν
        let rs = RuleSet::tensor();
        let expr = mul(
            tensor("g", vec![upper("mu"), upper("kappa")]),
            tensor("g", vec![lower("kappa"), lower("nu")]),
        );

        let result = rs
            .iter()
            .find(|r| r.name == "metric_inverse_right")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(tensor("δ", vec![upper("mu"), lower("nu")])));
    }

    #[test]
    fn metric_inverse_left() {
        // g_κν * g^μκ = δ^μ_ν
        let rs = RuleSet::tensor();
        let expr = mul(
            tensor("g", vec![lower("kappa"), lower("nu")]),
            tensor("g", vec![upper("mu"), upper("kappa")]),
        );

        let result = rs
            .iter()
            .find(|r| r.name == "metric_inverse_left")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(tensor("δ", vec![upper("mu"), lower("nu")])));
    }

    #[test]
    fn metric_no_match_wrong_positions() {
        // g_μν * v_ν should NOT match metric_lower (expects v^ν not v_ν)
        let rs = RuleSet::tensor();
        let expr = mul(
            tensor("g", vec![lower("mu"), lower("nu")]),
            tensor("v", vec![lower("nu")]), // Wrong: should be upper
        );

        let result = rs
            .iter()
            .find(|r| r.name == "metric_lower_right")
            .and_then(|r| r.apply_ltr(&expr));

        assert!(result.is_none());
    }

    #[test]
    fn metric_no_match_different_indices() {
        // g_μν * v^σ should NOT match (indices don't contract)
        let rs = RuleSet::tensor();
        let expr = mul(
            tensor("g", vec![lower("mu"), lower("nu")]),
            tensor("v", vec![upper("sigma")]), // Different index
        );

        let result = rs
            .iter()
            .find(|r| r.name == "metric_lower_right")
            .and_then(|r| r.apply_ltr(&expr));

        assert!(result.is_none());
    }

    // === Tensor symmetry rule tests ===

    #[test]
    fn metric_symmetric_lower() {
        // g_μν = g_νμ (covariant metric is symmetric)
        let rs = RuleSet::tensor();
        let expr = tensor("g", vec![lower("mu"), lower("nu")]);

        let result = rs
            .iter()
            .find(|r| r.name == "metric_symmetric_lower")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(tensor("g", vec![lower("nu"), lower("mu")])));
    }

    #[test]
    fn metric_symmetric_upper() {
        // g^μν = g^νμ (contravariant metric is symmetric)
        let rs = RuleSet::tensor();
        let expr = tensor("g", vec![upper("mu"), upper("nu")]);

        let result = rs
            .iter()
            .find(|r| r.name == "metric_symmetric_upper")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(tensor("g", vec![upper("nu"), upper("mu")])));
    }

    #[test]
    fn metric_symmetry_roundtrip() {
        // Applying symmetry twice returns to original
        let rs = RuleSet::tensor();
        let original = tensor("g", vec![lower("alpha"), lower("beta")]);

        let rule = rs
            .iter()
            .find(|r| r.name == "metric_symmetric_lower")
            .unwrap();

        let swapped = rule.apply_ltr(&original).unwrap();
        assert_eq!(swapped, tensor("g", vec![lower("beta"), lower("alpha")]));

        let back = rule.apply_ltr(&swapped).unwrap();
        assert_eq!(back, original);
    }

    // === Antisymmetric tensor rule tests ===

    #[test]
    fn levi_civita_antisymmetric_lower() {
        // ε_μν = -ε_νμ
        let rs = RuleSet::tensor();
        let expr = tensor("ε", vec![lower("mu"), lower("nu")]);

        let result = rs
            .iter()
            .find(|r| r.name == "levi_civita_antisymmetric_lower")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(
            result,
            Some(neg(tensor("ε", vec![lower("nu"), lower("mu")])))
        );
    }

    #[test]
    fn levi_civita_antisymmetric_upper() {
        // ε^μν = -ε^νμ
        let rs = RuleSet::tensor();
        let expr = tensor("ε", vec![upper("mu"), upper("nu")]);

        let result = rs
            .iter()
            .find(|r| r.name == "levi_civita_antisymmetric_upper")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(
            result,
            Some(neg(tensor("ε", vec![upper("nu"), upper("mu")])))
        );
    }

    #[test]
    fn em_field_antisymmetric() {
        // F^μν = -F^νμ (electromagnetic field tensor)
        let rs = RuleSet::tensor();
        let expr = tensor("F", vec![upper("mu"), upper("nu")]);

        let result = rs
            .iter()
            .find(|r| r.name == "em_field_antisymmetric_upper")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(
            result,
            Some(neg(tensor("F", vec![upper("nu"), upper("mu")])))
        );
    }

    // === Generic symmetry/antisymmetry helper tests ===

    #[test]
    fn add_symmetric_lower_custom_tensor() {
        // Test adding symmetry for a custom tensor "h" (e.g., metric perturbation)
        let mut rs = RuleSet::new();
        rs.add_symmetric_lower("h");

        let expr = tensor("h", vec![lower("mu"), lower("nu")]);
        let result = rs
            .iter()
            .find(|r| r.name == "h_symmetric_lower")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(tensor("h", vec![lower("nu"), lower("mu")])));
    }

    #[test]
    fn add_symmetric_upper_custom_tensor() {
        let mut rs = RuleSet::new();
        rs.add_symmetric_upper("T");

        let expr = tensor("T", vec![upper("a"), upper("b")]);
        let result = rs
            .iter()
            .find(|r| r.name == "T_symmetric_upper")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(tensor("T", vec![upper("b"), upper("a")])));
    }

    #[test]
    fn add_symmetric_both_positions() {
        let mut rs = RuleSet::new();
        rs.add_symmetric("S");

        // Should have both lower and upper rules
        assert!(rs.iter().any(|r| r.name == "S_symmetric_lower"));
        assert!(rs.iter().any(|r| r.name == "S_symmetric_upper"));
    }

    #[test]
    fn add_antisymmetric_lower_custom_tensor() {
        // Test adding antisymmetry for a custom tensor "ω" (e.g., vorticity)
        let mut rs = RuleSet::new();
        rs.add_antisymmetric_lower("ω");

        let expr = tensor("ω", vec![lower("i"), lower("j")]);
        let result = rs
            .iter()
            .find(|r| r.name == "ω_antisymmetric_lower")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(neg(tensor("ω", vec![lower("j"), lower("i")]))));
    }

    #[test]
    fn add_antisymmetric_upper_custom_tensor() {
        let mut rs = RuleSet::new();
        rs.add_antisymmetric_upper("B");

        let expr = tensor("B", vec![upper("i"), upper("j")]);
        let result = rs
            .iter()
            .find(|r| r.name == "B_antisymmetric_upper")
            .and_then(|r| r.apply_ltr(&expr));

        assert_eq!(result, Some(neg(tensor("B", vec![upper("j"), upper("i")]))));
    }

    #[test]
    fn add_antisymmetric_both_positions() {
        let mut rs = RuleSet::new();
        rs.add_antisymmetric("A");

        // Should have both lower and upper rules
        assert!(rs.iter().any(|r| r.name == "A_antisymmetric_lower"));
        assert!(rs.iter().any(|r| r.name == "A_antisymmetric_upper"));
    }

    #[test]
    fn chained_symmetry_methods() {
        // Test that methods can be chained
        let mut rs = RuleSet::new();
        rs.add_symmetric("g")
            .add_antisymmetric("F")
            .add_symmetric_lower("h");

        assert_eq!(rs.len(), 5); // 2 for g, 2 for F, 1 for h
    }
}
