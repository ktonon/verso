use crate::dim::Dimension;
use crate::eval::{free_vars, spot_check};
use crate::expr::Expr;
use crate::rule::{self, Pattern, Rule, RuleSet};
use crate::search;
use std::collections::{HashMap, HashSet};

/// Map from variable name to its declared dimension.
pub type DimEnv = HashMap<String, Dimension>;

/// Mathematical context accumulating declarations and verified results.
///
/// Used by both `verso_doc` (document verification) and the repl.
/// A user-defined function: name, parameter names, body expression.
#[derive(Debug, Clone)]
pub struct FuncDef {
    pub params: Vec<String>,
    pub body: Expr,
}

pub struct Context {
    pub rules: RuleSet,
    pub dims: DimEnv,
    pub consts: HashMap<String, Expr>,
    pub funcs: HashMap<String, FuncDef>,
}

impl Context {
    pub fn new() -> Self {
        Context {
            rules: RuleSet::full(),
            dims: DimEnv::new(),
            consts: HashMap::new(),
            funcs: HashMap::new(),
        }
    }

    /// Declare a variable with optional dimensions.
    pub fn declare_var(&mut self, name: &str, dim: Option<Dimension>) {
        if let Some(d) = dim {
            self.dims.insert(name.to_string(), d);
        }
    }

    /// Declare a named constant with a value expression.
    pub fn declare_const(&mut self, name: &str, value: Expr) {
        self.consts.insert(name.to_string(), value);
    }

    /// Declare a user-defined function.
    pub fn declare_func(&mut self, name: &str, params: Vec<String>, body: Expr) {
        self.funcs.insert(name.to_string(), FuncDef { params, body });
    }

    /// Apply constant substitution and function expansion to an expression.
    /// Function bodies may reference constants, so we substitute again after expansion.
    pub fn apply_consts(&self, expr: &Expr) -> Expr {
        let expr = substitute_consts(expr, &self.consts);
        let expr = expand_funcs(&expr, &self.funcs);
        if self.funcs.is_empty() {
            expr
        } else {
            substitute_consts(&expr, &self.consts)
        }
    }

    /// Register a verified claim as a bidirectional rewrite rule.
    /// Free variables in the claim become pattern wildcards.
    pub fn add_claim_as_rule(&mut self, name: &str, lhs: &Expr, rhs: &Expr) {
        let mut vars = free_vars(lhs);
        vars.extend(free_vars(rhs));
        let wildcards: HashSet<String> = vars.into_iter().collect();
        let lhs_pat = expr_to_pattern(lhs, &wildcards);
        let rhs_pat = expr_to_pattern(rhs, &wildcards);
        self.rules.add(Rule {
            name: format!("claim:{}", name),
            lhs: lhs_pat,
            rhs: rhs_pat,
            reversible: false,
        });
    }

    /// Check whether the dimension environment has any declarations.
    pub fn has_dims(&self) -> bool {
        !self.dims.is_empty()
    }

    /// Simplify an expression using the current rule set.
    /// Constants are substituted before simplification.
    pub fn simplify(&self, expr: &Expr) -> Expr {
        let expr = self.apply_consts(expr);
        search::simplify(&expr, &self.rules)
    }

    /// Check if two expressions are symbolically equal.
    /// Constants are substituted before comparison.
    /// Falls back to numerical spot-checking if symbolic fails.
    pub fn check_equal(&self, lhs: &Expr, rhs: &Expr) -> EqualityResult {
        let lhs = self.apply_consts(lhs);
        let rhs = self.apply_consts(rhs);
        let diff = Expr::Add(
            Box::new(lhs.clone()),
            Box::new(Expr::Neg(Box::new(rhs.clone()))),
        );
        let residual = search::simplify(&diff, &self.rules);

        if is_zero(&residual) {
            return EqualityResult::Equal;
        }

        match spot_check(&lhs, &rhs, SPOT_CHECK_SAMPLES) {
            Ok(()) => EqualityResult::NumericallyEqual {
                samples: SPOT_CHECK_SAMPLES,
                residual,
            },
            Err(_) => EqualityResult::NotEqual { residual },
        }
    }

    /// Check if two expressions are equivalent (simplified diff is zero).
    pub fn exprs_equivalent(&self, a: &Expr, b: &Expr) -> bool {
        if a == b {
            return true;
        }
        let diff = Expr::Add(
            Box::new(a.clone()),
            Box::new(Expr::Neg(Box::new(b.clone()))),
        );
        is_zero(&search::simplify(&diff, &self.rules))
    }

    /// Try applying a named rule at every subexpression of `from`
    /// and check if any result equals `to`.
    pub fn try_rule_produces(&self, from: &Expr, rule: &crate::rule::Rule, to: &Expr) -> bool {
        try_rule_produces_inner(from, rule, to, self)
    }

    /// Check dimensional consistency of a single expression.
    /// Constants and functions are expanded before checking.
    /// Always attempts inference so that explicit units and constants with
    /// units are checked even without :var declarations. Suppresses
    /// UndeclaredVar errors only when the expression has no type information
    /// at all (no units, no inline dimensions, no :var declarations).
    pub fn check_expr_dim(&self, expr: &Expr) -> Option<Result<Dimension, DimError>> {
        let expr = self.apply_consts(expr);
        match check_dim(&expr, &self.dims) {
            Ok(d) => Some(Ok(d)),
            Err(DimError::UndeclaredVar(_))
                if !self.has_dims() && !has_type_info(&expr) =>
            {
                None
            }
            Err(e) => Some(Err(e)),
        }
    }

    /// Infer the type (dimension) of an expression for display purposes.
    /// Always attempts inference — works with explicit units even without
    /// :var declarations. Returns None if dimensionless or inference fails.
    pub fn infer_type(&self, expr: &Expr) -> Option<Dimension> {
        let expr = self.apply_consts(expr);
        check_dim(&expr, &self.dims)
            .ok()
            .filter(|d| !d.is_dimensionless())
    }

    /// Check dimensional consistency of an equality.
    /// Constants and functions are expanded before checking.
    pub fn check_dims(
        &self,
        lhs: &Expr,
        rhs: &Expr,
    ) -> DimOutcome {
        let lhs = self.apply_consts(lhs);
        let rhs = self.apply_consts(rhs);
        check_claim_dim(&lhs, &rhs, &self.dims)
    }
}

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}

const SPOT_CHECK_SAMPLES: usize = 200;

/// Result of checking equality between two expressions.
#[derive(Debug)]
pub enum EqualityResult {
    Equal,
    NumericallyEqual { samples: usize, residual: Expr },
    NotEqual { residual: Expr },
}

impl EqualityResult {
    pub fn passed(&self) -> bool {
        matches!(self, EqualityResult::Equal | EqualityResult::NumericallyEqual { .. })
    }
}

/// Check if an expression is zero.
pub fn is_zero(expr: &Expr) -> bool {
    match expr {
        Expr::Rational(r) => r.is_zero(),
        Expr::FracPi(r) => r.is_zero(),
        _ => false,
    }
}

// --- Dimensional analysis (moved from verso_doc/src/dim.rs) ---

/// A dimensional analysis error.
#[derive(Debug)]
pub enum DimError {
    UndeclaredVar(String),
    Mismatch {
        expected: Dimension,
        got: Dimension,
        context: String,
    },
    NonDimensionlessFnArg {
        func: String,
        dim: Dimension,
    },
    NonIntegerPower,
}

impl std::fmt::Display for DimError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DimError::UndeclaredVar(v) => write!(f, "'{}' is typeless", v),
            DimError::Mismatch {
                expected,
                got,
                context,
            } => write!(
                f,
                "dimension mismatch in {}: expected {}, got {}",
                context, expected, got
            ),
            DimError::NonDimensionlessFnArg { func, dim } => {
                write!(
                    f,
                    "argument to {}() must be dimensionless, got {}",
                    func, dim
                )
            }
            DimError::NonIntegerPower => {
                write!(f, "cannot raise dimensional quantity to non-integer power")
            }
        }
    }
}

/// Result of dimension checking a claim.
#[derive(Debug)]
pub enum DimOutcome {
    /// All dimensions consistent.
    Pass,
    /// Some variables lack declarations — check skipped.
    Skipped { undeclared: Vec<String> },
    /// Dimension mismatch between lhs and rhs.
    LhsRhsMismatch { lhs: Dimension, rhs: Dimension },
    /// Error within an expression (e.g., adding length to time).
    ExprError { side: String, error: DimError },
}

impl DimOutcome {
    pub fn passed(&self) -> bool {
        matches!(self, DimOutcome::Pass | DimOutcome::Skipped { .. })
    }
}

/// Infer the dimension of an expression given a dimension environment.
pub fn check_dim(expr: &Expr, env: &DimEnv) -> Result<Dimension, DimError> {
    match expr {
        Expr::Rational(_) | Expr::FracPi(_) | Expr::Named(_) => Ok(Dimension::dimensionless()),
        Expr::Quantity(_inner, unit) => Ok(unit.dimension.clone()),
        Expr::Var { name, dim, .. } => {
            if let Some(d) = dim {
                return Ok(d.clone());
            }
            env.get(name)
                .cloned()
                .ok_or_else(|| DimError::UndeclaredVar(name.clone()))
        }
        Expr::Add(a, b) => {
            let da = match check_dim(a, env) {
                Ok(d) => d,
                Err(DimError::UndeclaredVar(v)) => {
                    if let Ok(db) = check_dim(b, env) {
                        if !db.is_dimensionless() {
                            return Err(DimError::Mismatch {
                                expected: db,
                                got: Dimension::dimensionless(),
                                context: "addition".to_string(),
                            });
                        }
                    }
                    return Err(DimError::UndeclaredVar(v));
                }
                Err(e) => return Err(e),
            };
            let db = match check_dim(b, env) {
                Ok(d) => d,
                Err(DimError::UndeclaredVar(v)) => {
                    if !da.is_dimensionless() {
                        return Err(DimError::Mismatch {
                            expected: da,
                            got: Dimension::dimensionless(),
                            context: "addition".to_string(),
                        });
                    }
                    return Err(DimError::UndeclaredVar(v));
                }
                Err(e) => return Err(e),
            };
            if da != db {
                return Err(DimError::Mismatch {
                    expected: da,
                    got: db,
                    context: "addition".to_string(),
                });
            }
            Ok(da)
        }
        Expr::Mul(a, b) => {
            let da = check_dim(a, env)?;
            let db = check_dim(b, env)?;
            Ok(da.mul(&db))
        }
        Expr::Neg(inner) => check_dim(inner, env),
        Expr::Inv(inner) => {
            let d = check_dim(inner, env)?;
            Ok(d.inv())
        }
        Expr::Pow(base, exp) => {
            let db = check_dim(base, env)?;
            if db.is_dimensionless() {
                let de = check_dim(exp, env)?;
                if !de.is_dimensionless() {
                    return Err(DimError::Mismatch {
                        expected: Dimension::dimensionless(),
                        got: de,
                        context: "exponent".to_string(),
                    });
                }
                return Ok(Dimension::dimensionless());
            }
            let n = expr_as_integer(exp).ok_or(DimError::NonIntegerPower)?;
            Ok(db.pow(n))
        }
        Expr::Fn(kind, arg) => {
            let da = check_dim(arg, env)?;
            if !da.is_dimensionless() {
                return Err(DimError::NonDimensionlessFnArg {
                    func: format!("{:?}", kind).to_lowercase(),
                    dim: da,
                });
            }
            Ok(Dimension::dimensionless())
        }
        Expr::FnN(kind, args) => {
            for arg in args {
                let da = check_dim(arg, env)?;
                if !da.is_dimensionless() {
                    return Err(DimError::NonDimensionlessFnArg {
                        func: format!("{:?}", kind).to_lowercase(),
                        dim: da,
                    });
                }
            }
            Ok(Dimension::dimensionless())
        }
    }
}

/// Check that two sides of a claim have the same dimension.
pub fn check_claim_dim(lhs: &Expr, rhs: &Expr, env: &DimEnv) -> DimOutcome {
    let dl = match check_dim(lhs, env) {
        Ok(d) => d,
        Err(DimError::UndeclaredVar(v)) => {
            let mut undeclared = collect_undeclared(lhs, env);
            undeclared.extend(collect_undeclared(rhs, env));
            undeclared.sort();
            undeclared.dedup();
            if undeclared.is_empty() {
                undeclared.push(v);
            }
            return DimOutcome::Skipped { undeclared };
        }
        Err(e) => {
            return DimOutcome::ExprError {
                side: "lhs".to_string(),
                error: e,
            }
        }
    };

    let dr = match check_dim(rhs, env) {
        Ok(d) => d,
        Err(DimError::UndeclaredVar(v)) => {
            let mut undeclared = collect_undeclared(rhs, env);
            if undeclared.is_empty() {
                undeclared.push(v);
            }
            return DimOutcome::Skipped { undeclared };
        }
        Err(e) => {
            return DimOutcome::ExprError {
                side: "rhs".to_string(),
                error: e,
            }
        }
    };

    if dl == dr {
        DimOutcome::Pass
    } else {
        DimOutcome::LhsRhsMismatch { lhs: dl, rhs: dr }
    }
}

fn collect_undeclared(expr: &Expr, env: &DimEnv) -> Vec<String> {
    let mut undeclared = Vec::new();
    collect_undeclared_inner(expr, env, &mut undeclared);
    undeclared.sort();
    undeclared.dedup();
    undeclared
}

fn collect_undeclared_inner(expr: &Expr, env: &DimEnv, out: &mut Vec<String>) {
    match expr {
        Expr::Var { name, dim, .. } => {
            if dim.is_none() && !env.contains_key(name) {
                out.push(name.clone());
            }
        }
        Expr::Add(a, b) | Expr::Mul(a, b) | Expr::Pow(a, b) => {
            collect_undeclared_inner(a, env, out);
            collect_undeclared_inner(b, env, out);
        }
        Expr::Neg(inner) | Expr::Inv(inner) | Expr::Fn(_, inner) => {
            collect_undeclared_inner(inner, env, out);
        }
        Expr::FnN(_, args) => {
            for arg in args {
                collect_undeclared_inner(arg, env, out);
            }
        }
        Expr::Rational(_) | Expr::FracPi(_) | Expr::Named(_) => {}
        Expr::Quantity(inner, _) => {
            collect_undeclared_inner(inner, env, out);
        }
    }
}

/// Collect all unit display names from Quantity nodes in an expression.
pub fn collect_units(expr: &Expr) -> Vec<String> {
    expr.collect_units()
}

/// Check if an expression contains any type information (units or inline dimensions).
fn has_type_info(expr: &Expr) -> bool {
    match expr {
        Expr::Quantity(_, _) => true,
        Expr::Var { dim: Some(_), .. } => true,
        Expr::Add(a, b) | Expr::Mul(a, b) | Expr::Pow(a, b) => {
            has_type_info(a) || has_type_info(b)
        }
        Expr::Neg(inner) | Expr::Inv(inner) | Expr::Fn(_, inner) => has_type_info(inner),
        Expr::FnN(_, args) => args.iter().any(has_type_info),
        _ => false,
    }
}

fn expr_as_integer(expr: &Expr) -> Option<i32> {
    match expr {
        Expr::Rational(r) => {
            if r.den() == 1 {
                Some(r.num() as i32)
            } else {
                None
            }
        }
        Expr::Neg(inner) => expr_as_integer(inner).map(|n| -n),
        _ => None,
    }
}

/// Convert an Expr into a Pattern, turning free variables into wildcards.
fn expr_to_pattern(expr: &Expr, wildcards: &HashSet<String>) -> Pattern {
    match expr {
        Expr::Var { name, .. } => {
            if wildcards.contains(name) {
                Pattern::Wildcard(name.clone())
            } else {
                rule::p_var(name, vec![])
            }
        }
        Expr::Rational(r) => Pattern::Rational(*r),
        Expr::FracPi(r) => Pattern::FracPi(*r),
        Expr::Named(n) => Pattern::Named(*n),
        Expr::Add(a, b) => Pattern::Add(
            Box::new(expr_to_pattern(a, wildcards)),
            Box::new(expr_to_pattern(b, wildcards)),
        ),
        Expr::Mul(a, b) => Pattern::Mul(
            Box::new(expr_to_pattern(a, wildcards)),
            Box::new(expr_to_pattern(b, wildcards)),
        ),
        Expr::Pow(a, b) => Pattern::Pow(
            Box::new(expr_to_pattern(a, wildcards)),
            Box::new(expr_to_pattern(b, wildcards)),
        ),
        Expr::Neg(inner) => Pattern::Neg(Box::new(expr_to_pattern(inner, wildcards))),
        Expr::Inv(inner) => Pattern::Inv(Box::new(expr_to_pattern(inner, wildcards))),
        Expr::Fn(kind, inner) => {
            Pattern::Fn(kind.clone(), Box::new(expr_to_pattern(inner, wildcards)))
        }
        Expr::FnN(kind, args) => Pattern::FnN(
            kind.clone(),
            args.iter().map(|a| expr_to_pattern(a, wildcards)).collect(),
        ),
        Expr::Quantity(inner, _unit) => {
            // Quantities lose their unit in patterns — match on the inner expression
            expr_to_pattern(inner, wildcards)
        }
    }
}

/// Substitute all constants in an expression.
/// Replaces `Var { name, .. }` nodes whose name appears in `consts` with the constant value.
pub fn substitute_consts(expr: &Expr, consts: &HashMap<String, Expr>) -> Expr {
    if consts.is_empty() {
        return expr.clone();
    }
    match expr {
        Expr::Var { name, .. } => {
            if let Some(value) = consts.get(name) {
                value.clone()
            } else {
                expr.clone()
            }
        }
        Expr::Add(a, b) => Expr::Add(
            Box::new(substitute_consts(a, consts)),
            Box::new(substitute_consts(b, consts)),
        ),
        Expr::Mul(a, b) => Expr::Mul(
            Box::new(substitute_consts(a, consts)),
            Box::new(substitute_consts(b, consts)),
        ),
        Expr::Pow(a, b) => Expr::Pow(
            Box::new(substitute_consts(a, consts)),
            Box::new(substitute_consts(b, consts)),
        ),
        Expr::Neg(inner) => Expr::Neg(Box::new(substitute_consts(inner, consts))),
        Expr::Inv(inner) => Expr::Inv(Box::new(substitute_consts(inner, consts))),
        Expr::Fn(kind, inner) => Expr::Fn(kind.clone(), Box::new(substitute_consts(inner, consts))),
        Expr::FnN(kind, args) => Expr::FnN(
            kind.clone(),
            args.iter().map(|a| substitute_consts(a, consts)).collect(),
        ),
        Expr::Quantity(inner, unit) => {
            Expr::Quantity(Box::new(substitute_consts(inner, consts)), unit.clone())
        }
        Expr::Rational(_) | Expr::FracPi(_) | Expr::Named(_) => expr.clone(),
    }
}

/// Expand user-defined function calls in an expression.
/// Replaces `Fn(Custom(name), arg)` / `FnN(Custom(name), args)` with the function body,
/// substituting parameters with the provided arguments.
fn expand_funcs(expr: &Expr, funcs: &HashMap<String, FuncDef>) -> Expr {
    if funcs.is_empty() {
        return expr.clone();
    }
    match expr {
        Expr::Fn(crate::expr::FnKind::Custom(name), arg) => {
            if let Some(def) = funcs.get(name) {
                let expanded_arg = expand_funcs(arg, funcs);
                let mut bindings = HashMap::new();
                if let Some(param) = def.params.first() {
                    bindings.insert(param.clone(), expanded_arg);
                }
                let result = substitute_consts(&def.body, &bindings);
                expand_funcs(&result, funcs)
            } else {
                Expr::Fn(
                    crate::expr::FnKind::Custom(name.clone()),
                    Box::new(expand_funcs(arg, funcs)),
                )
            }
        }
        Expr::FnN(crate::expr::FnKind::Custom(name), args) => {
            if let Some(def) = funcs.get(name) {
                let expanded_args: Vec<Expr> = args.iter().map(|a| expand_funcs(a, funcs)).collect();
                let mut bindings = HashMap::new();
                for (param, arg) in def.params.iter().zip(expanded_args) {
                    bindings.insert(param.clone(), arg);
                }
                let result = substitute_consts(&def.body, &bindings);
                expand_funcs(&result, funcs)
            } else {
                Expr::FnN(
                    crate::expr::FnKind::Custom(name.clone()),
                    args.iter().map(|a| expand_funcs(a, funcs)).collect(),
                )
            }
        }
        Expr::Add(a, b) => Expr::Add(
            Box::new(expand_funcs(a, funcs)),
            Box::new(expand_funcs(b, funcs)),
        ),
        Expr::Mul(a, b) => Expr::Mul(
            Box::new(expand_funcs(a, funcs)),
            Box::new(expand_funcs(b, funcs)),
        ),
        Expr::Pow(a, b) => Expr::Pow(
            Box::new(expand_funcs(a, funcs)),
            Box::new(expand_funcs(b, funcs)),
        ),
        Expr::Neg(inner) => Expr::Neg(Box::new(expand_funcs(inner, funcs))),
        Expr::Inv(inner) => Expr::Inv(Box::new(expand_funcs(inner, funcs))),
        Expr::Fn(kind, inner) => Expr::Fn(kind.clone(), Box::new(expand_funcs(inner, funcs))),
        Expr::FnN(kind, args) => Expr::FnN(
            kind.clone(),
            args.iter().map(|a| expand_funcs(a, funcs)).collect(),
        ),
        Expr::Quantity(inner, unit) => {
            Expr::Quantity(Box::new(expand_funcs(inner, funcs)), unit.clone())
        }
        Expr::Rational(_) | Expr::FracPi(_) | Expr::Named(_) | Expr::Var { .. } => expr.clone(),
    }
}

fn try_rule_produces_inner(from: &Expr, rule: &crate::rule::Rule, to: &Expr, ctx: &Context) -> bool {
    if let Some(result) = rule.apply_ltr(from) {
        if ctx.exprs_equivalent(&result, to) {
            return true;
        }
    }
    if rule.reversible {
        if let Some(result) = rule.apply_rtl(from) {
            if ctx.exprs_equivalent(&result, to) {
                return true;
            }
        }
    }

    match from {
        Expr::Add(a, b) | Expr::Mul(a, b) | Expr::Pow(a, b) => {
            try_rule_produces_inner(a, rule, to, ctx)
                || try_rule_produces_inner(b, rule, to, ctx)
        }
        Expr::Neg(inner) | Expr::Inv(inner) | Expr::Fn(_, inner) => {
            try_rule_produces_inner(inner, rule, to, ctx)
        }
        Expr::FnN(_, args) => args.iter().any(|a| try_rule_produces_inner(a, rule, to, ctx)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dim::BaseDim;
    use crate::parser::parse_expr;

    #[test]
    fn check_expr_dim_catches_length_plus_time() {
        let mut ctx = Context::new();
        ctx.declare_var("b", Some(Dimension::single(BaseDim::L, 1)));
        let expr = parse_expr("b + 4 [s]").unwrap();
        let result = ctx.check_expr_dim(&expr);
        assert!(result.is_some());
        assert!(result.unwrap().is_err(), "adding length to time should fail");
    }

    #[test]
    fn check_expr_dim_ok_same_dimension() {
        let mut ctx = Context::new();
        ctx.declare_var("b", Some(Dimension::single(BaseDim::L, 1)));
        let expr = parse_expr("b + 4 [m]").unwrap();
        let result = ctx.check_expr_dim(&expr);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok(), "adding length to meters should pass");
    }

    #[test]
    fn check_expr_dim_typed_plus_untyped_is_error() {
        // x has no type, 4 [s] has type [T] → error
        let ctx = Context::new();
        let expr = parse_expr("x + 4 [s]").unwrap();
        let result = ctx.check_expr_dim(&expr);
        assert!(result.is_some(), "should not suppress when expression has units");
        assert!(result.unwrap().is_err());
    }

    #[test]
    fn check_expr_dim_substitutes_consts() {
        let mut ctx = Context::new();
        ctx.declare_var(
            "v",
            Some(Dimension::single(BaseDim::L, 1).mul(&Dimension::single(BaseDim::T, -1))),
        );
        ctx.declare_const("g", parse_expr("9.8").unwrap());
        // v + g should fail with Mismatch (not UndeclaredVar) after const substitution
        let expr = parse_expr("v + g").unwrap();
        let result = ctx.check_expr_dim(&expr);
        assert!(result.is_some());
        match result.unwrap() {
            Err(DimError::Mismatch { .. }) => {} // expected: [L T^-1] + [1]
            Err(DimError::UndeclaredVar(_)) => {
                panic!("const 'g' should be substituted before dim check")
            }
            other => panic!("expected Mismatch, got {:?}", other),
        }
    }

    #[test]
    fn check_dims_substitutes_consts() {
        let mut ctx = Context::new();
        ctx.declare_var(
            "v",
            Some(Dimension::single(BaseDim::L, 1).mul(&Dimension::single(BaseDim::T, -1))),
        );
        ctx.declare_const("c", parse_expr("5").unwrap());
        // v = c should be LhsRhsMismatch: [L T^-1] vs [1]
        let lhs = parse_expr("v").unwrap();
        let rhs = parse_expr("c").unwrap();
        let result = ctx.check_dims(&lhs, &rhs);
        match result {
            DimOutcome::Skipped { .. } => {
                panic!("should not skip dim check when const is a known substitution")
            }
            DimOutcome::LhsRhsMismatch { .. } => {} // expected
            other => panic!("expected LhsRhsMismatch, got {:?}", other),
        }
    }

    #[test]
    fn infer_type_with_declared_vars() {
        let mut ctx = Context::new();
        ctx.declare_var("a", Some(Dimension::single(BaseDim::L, 1)));
        ctx.declare_var("b", Some(Dimension::single(BaseDim::L, 1)));
        let expr = parse_expr("a + b").unwrap();
        assert_eq!(ctx.infer_type(&expr), Some(Dimension::single(BaseDim::L, 1)));
        let expr = parse_expr("a * b").unwrap();
        assert_eq!(ctx.infer_type(&expr), Some(Dimension::single(BaseDim::L, 2)));
    }

    #[test]
    fn infer_type_with_units_no_vars() {
        // No :var declarations — should still infer type from explicit units
        let ctx = Context::new();
        let expr = parse_expr("3 [m]").unwrap();
        assert_eq!(ctx.infer_type(&expr), Some(Dimension::single(BaseDim::L, 1)));
    }

    #[test]
    fn infer_type_dimensionless_returns_none() {
        let ctx = Context::new();
        let expr = parse_expr("3 + 2").unwrap();
        assert_eq!(ctx.infer_type(&expr), None);
    }

    #[test]
    fn infer_type_undeclared_var_returns_none() {
        let ctx = Context::new();
        let expr = parse_expr("x + 3").unwrap();
        assert_eq!(ctx.infer_type(&expr), None);
    }

    #[test]
    fn check_expr_dim_const_with_units_no_vars() {
        // No :var declarations, but const has units — should still catch mismatch
        let mut ctx = Context::new();
        ctx.declare_const("c", parse_expr("3 [m/s]").unwrap());
        let expr = parse_expr("c + 1").unwrap();
        let result = ctx.check_expr_dim(&expr);
        assert!(result.is_some(), "should check dims even without :var");
        assert!(result.unwrap().is_err(), "adding [m/s] to dimensionless should fail");
    }

    #[test]
    fn check_expr_dim_pure_units_no_vars() {
        // No declarations at all — explicit units should still be checked
        let ctx = Context::new();
        let expr = parse_expr("3 [m] + 4 [s]").unwrap();
        let result = ctx.check_expr_dim(&expr);
        assert!(result.is_some(), "should check dims for explicit units");
        assert!(result.unwrap().is_err(), "adding [m] to [s] should fail");
    }

    #[test]
    fn check_expr_dim_undeclared_var_no_vars_is_none() {
        // No declarations, bare variable — should not produce noise
        let ctx = Context::new();
        let expr = parse_expr("x + 3").unwrap();
        assert!(ctx.check_expr_dim(&expr).is_none());
    }

    #[test]
    fn check_expr_dim_undeclared_var_with_units_is_error() {
        // No :var declarations, but expression mixes units with typeless var → error
        let ctx = Context::new();
        let expr = parse_expr("1 [m] + 2 [km] + x").unwrap();
        let result = ctx.check_expr_dim(&expr);
        assert!(result.is_some(), "should not suppress when expression has units");
        assert!(result.unwrap().is_err());
    }

    #[test]
    fn typeless_var_in_typed_addition_reports_mismatch() {
        // Typeless var in typed addition should produce a dimension mismatch
        let ctx = Context::new();
        let expr = parse_expr("1 [m] + x").unwrap();
        let result = ctx.check_expr_dim(&expr);
        let err = result.unwrap().unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("dimension mismatch"),
            "error should be a dimension mismatch, got: {}",
            msg
        );
        assert!(
            msg.contains("[1]"),
            "typeless should show as [1], got: {}",
            msg
        );
    }

    #[test]
    fn check_expr_dim_inline_dim_with_undeclared_var_is_error() {
        // x has inline dim [L], y has no type → error
        let ctx = Context::new();
        let expr = parse_expr("x [L] + y").unwrap();
        let result = ctx.check_expr_dim(&expr);
        assert!(result.is_some(), "should not suppress when expression has inline dims");
        assert!(result.unwrap().is_err());
    }

    // --- check_equal branches ---

    #[test]
    fn check_equal_symbolically_equal() {
        let ctx = Context::new();
        let lhs = parse_expr("x + 0").unwrap();
        let rhs = parse_expr("x").unwrap();
        let result = ctx.check_equal(&lhs, &rhs);
        assert!(result.passed());
        assert!(matches!(result, EqualityResult::Equal));
    }

    #[test]
    fn check_equal_not_equal() {
        let ctx = Context::new();
        let lhs = parse_expr("x").unwrap();
        let rhs = parse_expr("x + 1").unwrap();
        let result = ctx.check_equal(&lhs, &rhs);
        assert!(!result.passed());
        assert!(matches!(result, EqualityResult::NotEqual { .. }));
    }

    #[test]
    fn check_equal_numerically_equal() {
        let ctx = Context::new();
        // sin(x)*cos(x) = sin(2*x)/2 — trig identity the simplifier can't prove but spot_check can
        let lhs = parse_expr("sin(x) * cos(x)").unwrap();
        let rhs = parse_expr("sin(2 * x) / 2").unwrap();
        let result = ctx.check_equal(&lhs, &rhs);
        assert!(result.passed());
        // If the simplifier can't prove it, it should fall back to numerical
        match result {
            EqualityResult::Equal | EqualityResult::NumericallyEqual { .. } => {}
            EqualityResult::NotEqual { .. } => panic!("should be equal"),
        }
    }

    // --- expand_funcs ---

    #[test]
    fn expand_single_arg_func() {
        use crate::expr::FnKind;
        let mut ctx = Context::new();
        // f(x) = x + 1
        ctx.declare_func("f", vec!["x".to_string()], parse_expr("x + 1").unwrap());
        // Build f(3) manually since the parser treats it as implicit multiplication
        let expr = Expr::Fn(FnKind::Custom("f".to_string()), Box::new(parse_expr("3").unwrap()));
        let expanded = ctx.apply_consts(&expr);
        assert_eq!(expanded, parse_expr("3 + 1").unwrap());
    }

    #[test]
    fn expand_multi_arg_func() {
        use crate::eval::eval_f64;
        let mut ctx = Context::new();
        // g(a, b) = a + b
        ctx.declare_func(
            "g",
            vec!["a".to_string(), "b".to_string()],
            parse_expr("a + b").unwrap(),
        );
        let expr = Expr::FnN(
            crate::expr::FnKind::Custom("g".to_string()),
            vec![parse_expr("2").unwrap(), parse_expr("3").unwrap()],
        );
        let expanded = ctx.apply_consts(&expr);
        assert_eq!(eval_f64(&expanded, &HashMap::new()), Some(5.0));
    }

    #[test]
    fn expand_unknown_custom_fn_unchanged() {
        use crate::expr::FnKind;
        let ctx = Context::new();
        let expr = Expr::Fn(
            FnKind::Custom("f".to_string()),
            Box::new(parse_expr("x").unwrap()),
        );
        let expanded = ctx.apply_consts(&expr);
        assert_eq!(expanded, expr);
    }

    // --- DimError Display ---

    #[test]
    fn dim_error_display_undeclared_var() {
        let err = DimError::UndeclaredVar("x".to_string());
        assert!(err.to_string().contains("x"));
        assert!(err.to_string().contains("typeless"));
    }

    #[test]
    fn dim_error_display_mismatch() {
        let err = DimError::Mismatch {
            expected: Dimension::single(BaseDim::L, 1),
            got: Dimension::single(BaseDim::T, 1),
            context: "addition".to_string(),
        };
        let s = err.to_string();
        assert!(s.contains("addition"));
        assert!(s.contains("mismatch"));
    }

    #[test]
    fn dim_error_display_non_dimensionless_fn_arg() {
        let err = DimError::NonDimensionlessFnArg {
            func: "sin".to_string(),
            dim: Dimension::single(BaseDim::L, 1),
        };
        let s = err.to_string();
        assert!(s.contains("sin"));
        assert!(s.contains("dimensionless"));
    }

    #[test]
    fn dim_error_display_non_integer_power() {
        let err = DimError::NonIntegerPower;
        assert!(err.to_string().contains("non-integer power"));
    }

    #[test]
    fn dim_error_display_typeless_as_mismatch() {
        // Typeless values show as [1] in mismatch errors
        let err = DimError::Mismatch {
            expected: Dimension::single(BaseDim::L, 1),
            got: Dimension::dimensionless(),
            context: "addition".to_string(),
        };
        let s = err.to_string();
        assert_eq!(s, "dimension mismatch in addition: expected [L], got [1]");
    }

    // --- DimOutcome::passed ---

    #[test]
    fn dim_outcome_passed_for_pass() {
        assert!(DimOutcome::Pass.passed());
    }

    #[test]
    fn dim_outcome_passed_for_skipped() {
        assert!(DimOutcome::Skipped {
            undeclared: vec!["x".into()]
        }
        .passed());
    }

    #[test]
    fn dim_outcome_not_passed_for_mismatch() {
        assert!(!DimOutcome::LhsRhsMismatch {
            lhs: Dimension::single(BaseDim::L, 1),
            rhs: Dimension::single(BaseDim::T, 1),
        }
        .passed());
    }

    #[test]
    fn dim_outcome_not_passed_for_expr_error() {
        assert!(!DimOutcome::ExprError {
            side: "lhs".to_string(),
            error: DimError::NonIntegerPower,
        }
        .passed());
    }

    // --- check_dim branches ---

    #[test]
    fn check_dim_mul() {
        let mut env = DimEnv::new();
        env.insert("a".into(), Dimension::single(BaseDim::L, 1));
        env.insert("b".into(), Dimension::single(BaseDim::T, -1));
        let expr = parse_expr("a * b").unwrap();
        let dim = check_dim(&expr, &env).unwrap();
        assert_eq!(
            dim,
            Dimension::single(BaseDim::L, 1).mul(&Dimension::single(BaseDim::T, -1))
        );
    }

    #[test]
    fn check_dim_inv() {
        let mut env = DimEnv::new();
        env.insert("t".into(), Dimension::single(BaseDim::T, 1));
        let expr = parse_expr("1/t").unwrap();
        let dim = check_dim(&expr, &env).unwrap();
        assert_eq!(dim, Dimension::single(BaseDim::T, -1));
    }

    #[test]
    fn check_dim_pow_dimensional_base() {
        let mut env = DimEnv::new();
        env.insert("x".into(), Dimension::single(BaseDim::L, 1));
        let expr = parse_expr("x^3").unwrap();
        let dim = check_dim(&expr, &env).unwrap();
        assert_eq!(dim, Dimension::single(BaseDim::L, 3));
    }

    #[test]
    fn check_dim_pow_non_integer_exponent_error() {
        let mut env = DimEnv::new();
        env.insert("x".into(), Dimension::single(BaseDim::L, 1));
        // x^(1/3) — non-integer power of dimensional quantity
        let expr = parse_expr("x^(1/3)").unwrap();
        let result = check_dim(&expr, &env);
        assert!(matches!(result, Err(DimError::NonIntegerPower)));
    }

    #[test]
    fn check_dim_pow_dimensional_exponent_error() {
        let mut env = DimEnv::new();
        env.insert("x".into(), Dimension::single(BaseDim::T, 1));
        // 2^x where x has dimension [T] — exponent must be dimensionless
        let expr = parse_expr("2^x").unwrap();
        let result = check_dim(&expr, &env);
        assert!(matches!(result, Err(DimError::Mismatch { .. })));
    }

    #[test]
    fn check_dim_fn_dimensional_arg_error() {
        let mut env = DimEnv::new();
        env.insert("x".into(), Dimension::single(BaseDim::L, 1));
        let expr = parse_expr("sin(x)").unwrap();
        let result = check_dim(&expr, &env);
        assert!(matches!(
            result,
            Err(DimError::NonDimensionlessFnArg { .. })
        ));
    }

    #[test]
    fn check_dim_fnn_dimensional_arg_error() {
        let mut env = DimEnv::new();
        env.insert("x".into(), Dimension::single(BaseDim::L, 1));
        let expr = Expr::FnN(
            crate::expr::FnKind::Min,
            vec![parse_expr("x").unwrap(), parse_expr("1").unwrap()],
        );
        let result = check_dim(&expr, &env);
        assert!(matches!(
            result,
            Err(DimError::NonDimensionlessFnArg { .. })
        ));
    }

    // --- check_claim_dim paths ---

    #[test]
    fn check_claim_dim_rhs_undeclared_skips() {
        let mut env = DimEnv::new();
        env.insert("a".into(), Dimension::single(BaseDim::L, 1));
        // lhs declared, rhs undeclared
        let lhs = parse_expr("a").unwrap();
        let rhs = parse_expr("b").unwrap();
        let result = check_claim_dim(&lhs, &rhs, &env);
        assert!(matches!(result, DimOutcome::Skipped { .. }));
    }

    #[test]
    fn check_claim_dim_rhs_expr_error() {
        let mut env = DimEnv::new();
        env.insert("a".into(), Dimension::single(BaseDim::L, 1));
        env.insert("b".into(), Dimension::single(BaseDim::T, 1));
        // rhs has internal dim error: a + b (length + time)
        let lhs = parse_expr("a").unwrap();
        let rhs = parse_expr("a + b").unwrap();
        let result = check_claim_dim(&lhs, &rhs, &env);
        assert!(matches!(
            result,
            DimOutcome::ExprError {
                side,
                ..
            } if side == "rhs"
        ));
    }

    #[test]
    fn check_claim_dim_lhs_expr_error() {
        let mut env = DimEnv::new();
        env.insert("a".into(), Dimension::single(BaseDim::L, 1));
        env.insert("b".into(), Dimension::single(BaseDim::T, 1));
        let lhs = parse_expr("a + b").unwrap();
        let rhs = parse_expr("a").unwrap();
        let result = check_claim_dim(&lhs, &rhs, &env);
        assert!(matches!(
            result,
            DimOutcome::ExprError {
                side,
                ..
            } if side == "lhs"
        ));
    }

    // --- expr_as_integer ---

    #[test]
    fn expr_as_integer_rational() {
        assert_eq!(expr_as_integer(&parse_expr("5").unwrap()), Some(5));
    }

    #[test]
    fn expr_as_integer_negative() {
        assert_eq!(expr_as_integer(&parse_expr("-3").unwrap()), Some(-3));
    }

    #[test]
    fn expr_as_integer_fraction_returns_none() {
        assert_eq!(expr_as_integer(&parse_expr("1/2").unwrap()), None);
    }

    #[test]
    fn expr_as_integer_var_returns_none() {
        assert_eq!(expr_as_integer(&parse_expr("x").unwrap()), None);
    }

    // --- exprs_equivalent ---

    #[test]
    fn exprs_equivalent_identical() {
        let ctx = Context::new();
        let a = parse_expr("x").unwrap();
        assert!(ctx.exprs_equivalent(&a, &a));
    }

    #[test]
    fn exprs_equivalent_simplified() {
        let ctx = Context::new();
        let a = parse_expr("x + 0").unwrap();
        let b = parse_expr("x").unwrap();
        assert!(ctx.exprs_equivalent(&a, &b));
    }

    // --- is_zero ---

    #[test]
    fn is_zero_rational() {
        assert!(is_zero(&parse_expr("0").unwrap()));
    }

    #[test]
    fn is_zero_non_zero() {
        assert!(!is_zero(&parse_expr("1").unwrap()));
    }

    #[test]
    fn is_zero_var() {
        assert!(!is_zero(&parse_expr("x").unwrap()));
    }
}
