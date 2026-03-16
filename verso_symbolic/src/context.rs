use crate::dim::Dimension;
use crate::eval::{free_vars, spot_check};
use crate::expr::{Expr, ExprKind, Span, Ty};
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
        self.funcs
            .insert(name.to_string(), FuncDef { params, body });
    }

    /// Apply const/function expansion and elaborate explicit type state onto the tree.
    pub fn elaborate_expr(&self, expr: &Expr) -> Result<Expr, DimError> {
        let expr = self.apply_consts(expr);
        elaborate_expr(&expr, &self.dims)
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
        let expr = elaborate_expr(&expr, &self.dims).unwrap_or(expr);
        search::simplify(&expr, &self.rules)
    }

    /// Check if two expressions are symbolically equal.
    /// Constants are substituted before comparison.
    /// Falls back to numerical spot-checking if symbolic fails.
    pub fn check_equal(&self, lhs: &Expr, rhs: &Expr) -> EqualityResult {
        let lhs = self.apply_consts(lhs);
        let rhs = self.apply_consts(rhs);
        let lhs = elaborate_expr(&lhs, &self.dims).unwrap_or(lhs);
        let rhs = elaborate_expr(&rhs, &self.dims).unwrap_or(rhs);
        let diff = Expr::derived(ExprKind::Add(
            Box::new(lhs.clone()),
            Box::new(Expr::derived(ExprKind::Neg(Box::new(rhs.clone())))),
        ));
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
        let a = self.apply_consts(a);
        let b = self.apply_consts(b);
        let a = elaborate_expr(&a, &self.dims).unwrap_or(a);
        let b = elaborate_expr(&b, &self.dims).unwrap_or(b);
        let diff = Expr::derived(ExprKind::Add(
            Box::new(a),
            Box::new(Expr::derived(ExprKind::Neg(Box::new(b)))),
        ));
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
            Err(DimError::UndeclaredVar(_, _)) if !self.has_dims() && !has_type_info(&expr) => None,
            Err(e) => Some(Err(e)),
        }
    }

    /// Infer the explicit type state of an expression for display/consumer use.
    /// Always attempts inference — works with explicit units even without
    /// :var declarations. Returns None only when elaboration fails.
    pub fn infer_ty(&self, expr: &Expr) -> Option<Ty> {
        let expr = self.elaborate_expr(expr).ok()?;
        Some(expr.ty)
    }

    /// Infer the non-dimensionless physical dimension of an expression.
    /// Returns None if the type is unresolved, dimensionless, or inference fails.
    pub fn infer_type(&self, expr: &Expr) -> Option<Dimension> {
        match self.infer_ty(expr)? {
            Ty::Concrete(dim) if !dim.is_dimensionless() => Some(dim),
            _ => None,
        }
    }

    /// Check dimensional consistency of an equality.
    /// Constants and functions are expanded before checking.
    pub fn check_dims(&self, lhs: &Expr, rhs: &Expr) -> DimOutcome {
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
        matches!(
            self,
            EqualityResult::Equal | EqualityResult::NumericallyEqual { .. }
        )
    }
}

/// Check if an expression is zero.
pub fn is_zero(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Rational(r) => r.is_zero(),
        ExprKind::FracPi(r) => r.is_zero(),
        _ => false,
    }
}

// --- Dimensional analysis (moved from verso_doc/src/dim.rs) ---

/// A dimensional analysis error.
#[derive(Debug)]
pub enum DimError {
    UndeclaredVar(String, Span),
    Mismatch {
        expected: Dimension,
        got: Dimension,
        context: String,
        span: Span,
    },
    NonDimensionlessFnArg {
        func: String,
        dim: Dimension,
        span: Span,
    },
    NonIntegerPower(Span),
}

impl std::fmt::Display for DimError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DimError::UndeclaredVar(v, _) => write!(f, "'{}' is typeless", v),
            DimError::Mismatch {
                expected,
                got,
                context,
                ..
            } => write!(
                f,
                "dimension mismatch in {}: expected {}, got {}",
                context, expected, got
            ),
            DimError::NonDimensionlessFnArg { func, dim, .. } => {
                write!(
                    f,
                    "argument to {}() must be dimensionless, got {}",
                    func, dim
                )
            }
            DimError::NonIntegerPower(_) => {
                write!(f, "cannot raise dimensional quantity to non-integer power")
            }
        }
    }
}

impl DimError {
    /// Return the source span associated with this error.
    pub fn span(&self) -> Span {
        match self {
            DimError::UndeclaredVar(_, span) => *span,
            DimError::Mismatch { span, .. } => *span,
            DimError::NonDimensionlessFnArg { span, .. } => *span,
            DimError::NonIntegerPower(span) => *span,
        }
    }
}

/// Format a dim error with a caret underline pointing at the offending span.
///
/// `source` is the parsed source text. `prefix_width` is the number of display
/// characters before the source on the prompt line (e.g. 2 for `"> "`).
///
/// Returns a two-line string: a caret row and the error message, both colored red.
pub fn format_dim_error(error: &DimError, source: &str, prefix_width: usize) -> String {
    let span = error.span();
    let start = span.start.min(source.chars().count());
    let end = span.end.min(source.chars().count());
    let caret_len = if end > start { end - start } else { 1 };
    format!(
        "\x1b[31m{:>width$}{}\ndim error: {}\x1b[0m",
        "",
        "^".repeat(caret_len),
        error,
        width = prefix_width + start,
    )
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
    let expr = elaborate_expr(expr, env)?;
    check_dim_typed(&expr)
}

fn check_dim_typed(expr: &Expr) -> Result<Dimension, DimError> {
    match &expr.kind {
        ExprKind::Rational(_) | ExprKind::FracPi(_) | ExprKind::Named(_) | ExprKind::Quantity(_, _) => {
            ty_dimension(&expr.ty).ok_or_else(|| {
                DimError::Mismatch {
                    expected: Dimension::dimensionless(),
                    got: Dimension::dimensionless(),
                    context: "typed expression".to_string(),
                    span: expr.span,
                }
            })
        }
        ExprKind::Var { name, .. } => ty_dimension(&expr.ty)
            .ok_or_else(|| DimError::UndeclaredVar(name.clone(), expr.span)),
        ExprKind::Add(a, b) => {
            let da = match check_dim_typed(a) {
                Ok(d) => d,
                Err(DimError::UndeclaredVar(v, span)) => {
                    if let Ok(db) = check_dim_typed(b) {
                        if !db.is_dimensionless() {
                            return Err(DimError::Mismatch {
                                expected: db,
                                got: Dimension::dimensionless(),
                                context: "addition".to_string(),
                                span: a.span,
                            });
                        }
                    }
                    return Err(DimError::UndeclaredVar(v, span));
                }
                Err(e) => return Err(e),
            };
            let db = match check_dim_typed(b) {
                Ok(d) => d,
                Err(DimError::UndeclaredVar(v, span)) => {
                    if !da.is_dimensionless() {
                        return Err(DimError::Mismatch {
                            expected: da,
                            got: Dimension::dimensionless(),
                            context: "addition".to_string(),
                            span: b.span,
                        });
                    }
                    return Err(DimError::UndeclaredVar(v, span));
                }
                Err(e) => return Err(e),
            };
            if da != db {
                return Err(DimError::Mismatch {
                    expected: da,
                    got: db,
                    context: "addition".to_string(),
                    span: b.span,
                });
            }
            Ok(da)
        }
        ExprKind::Mul(a, b) => {
            let da = check_dim_typed(a)?;
            let db = check_dim_typed(b)?;
            Ok(da.mul(&db))
        }
        ExprKind::Neg(inner) => check_dim_typed(inner),
        ExprKind::Inv(inner) => {
            let d = check_dim_typed(inner)?;
            Ok(d.inv())
        }
        ExprKind::Pow(base, exp) => {
            let db = check_dim_typed(base)?;
            if db.is_dimensionless() {
                let de = check_dim_typed(exp)?;
                if !de.is_dimensionless() {
                    return Err(DimError::Mismatch {
                        expected: Dimension::dimensionless(),
                        got: de,
                        context: "exponent".to_string(),
                        span: exp.span,
                    });
                }
                return Ok(Dimension::dimensionless());
            }
            let n = expr_as_integer(exp).ok_or(DimError::NonIntegerPower(exp.span))?;
            Ok(db.pow(n))
        }
        ExprKind::Fn(kind, arg) => {
            let da = check_dim_typed(arg)?;
            if !da.is_dimensionless() {
                return Err(DimError::NonDimensionlessFnArg {
                    func: format!("{:?}", kind).to_lowercase(),
                    dim: da,
                    span: arg.span,
                });
            }
            Ok(Dimension::dimensionless())
        }
        ExprKind::FnN(kind, args) => {
            for arg in args {
                let da = check_dim_typed(arg)?;
                if !da.is_dimensionless() {
                    return Err(DimError::NonDimensionlessFnArg {
                        func: format!("{:?}", kind).to_lowercase(),
                        dim: da,
                        span: arg.span,
                    });
                }
            }
            Ok(Dimension::dimensionless())
        }
    }
}

fn ty_dimension(ty: &Ty) -> Option<Dimension> {
    match ty {
        Ty::Concrete(dim) => Some(dim.clone()),
        Ty::Unresolved => None,
    }
}

fn elaborate_expr(expr: &Expr, env: &DimEnv) -> Result<Expr, DimError> {
    let span = expr.span;
    match &expr.kind {
        ExprKind::Rational(r) => Ok(Expr::spanned_typed(
            ExprKind::Rational(*r),
            span,
            Ty::Concrete(Dimension::dimensionless()),
        )),
        ExprKind::FracPi(r) => Ok(Expr::spanned_typed(
            ExprKind::FracPi(*r),
            span,
            Ty::Concrete(Dimension::dimensionless()),
        )),
        ExprKind::Named(n) => Ok(Expr::spanned_typed(
            ExprKind::Named(*n),
            span,
            Ty::Concrete(Dimension::dimensionless()),
        )),
        ExprKind::Quantity(inner, unit) => Ok(Expr::spanned_typed(
            ExprKind::Quantity(Box::new(elaborate_expr(inner, env)?), unit.clone()),
            span,
            Ty::Concrete(unit.dimension.clone()),
        )),
        ExprKind::Var { name, indices, dim } => {
            let ty = match &expr.ty {
                Ty::Concrete(dim) => Ty::Concrete(dim.clone()),
                Ty::Unresolved => match dim {
                    Some(dim) => Ty::Concrete(dim.clone()),
                    None => env
                        .get(name)
                        .cloned()
                        .map(Ty::Concrete)
                        .unwrap_or(Ty::Concrete(Dimension::dimensionless())),
                },
            };
            Ok(Expr::spanned_typed(
                ExprKind::Var {
                    name: name.clone(),
                    indices: indices.clone(),
                    dim: dim.clone(),
                },
                span,
                ty,
            ))
        }
        ExprKind::Add(a, b) => {
            let a = elaborate_expr(a, env)?;
            let b = elaborate_expr(b, env)?;
            let ty = match (ty_dimension(&a.ty), ty_dimension(&b.ty)) {
                (Some(da), Some(db)) if da == db => Ty::Concrete(da),
                (Some(da), Some(db)) => {
                    return Err(DimError::Mismatch {
                        expected: da,
                        got: db,
                        context: "addition".to_string(),
                        span: b.span,
                    });
                }
                (Some(da), None) if !da.is_dimensionless() => {
                    return Err(DimError::Mismatch {
                        expected: da,
                        got: Dimension::dimensionless(),
                        context: "addition".to_string(),
                        span: b.span,
                    });
                }
                (None, Some(db)) if !db.is_dimensionless() => {
                    return Err(DimError::Mismatch {
                        expected: db,
                        got: Dimension::dimensionless(),
                        context: "addition".to_string(),
                        span: a.span,
                    });
                }
                _ => Ty::Unresolved,
            };
            Ok(Expr::spanned_typed(
                ExprKind::Add(Box::new(a), Box::new(b)),
                span,
                ty,
            ))
        }
        ExprKind::Mul(a, b) => {
            let a = elaborate_expr(a, env)?;
            let b = elaborate_expr(b, env)?;
            let ty = match (ty_dimension(&a.ty), ty_dimension(&b.ty)) {
                (Some(da), Some(db)) => Ty::Concrete(da.mul(&db)),
                _ => Ty::Unresolved,
            };
            Ok(Expr::spanned_typed(
                ExprKind::Mul(Box::new(a), Box::new(b)),
                span,
                ty,
            ))
        }
        ExprKind::Neg(inner) => {
            let inner = elaborate_expr(inner, env)?;
            Ok(Expr::spanned_typed(
                ExprKind::Neg(Box::new(inner.clone())),
                span,
                inner.ty,
            ))
        }
        ExprKind::Inv(inner) => {
            let inner = elaborate_expr(inner, env)?;
            let ty = ty_dimension(&inner.ty)
                .map(|dim| Ty::Concrete(dim.inv()))
                .unwrap_or(Ty::Unresolved);
            Ok(Expr::spanned_typed(
                ExprKind::Inv(Box::new(inner)),
                span,
                ty,
            ))
        }
        ExprKind::Pow(base, exp) => {
            let base = elaborate_expr(base, env)?;
            let exp = elaborate_expr(exp, env)?;
            let ty = match (ty_dimension(&base.ty), ty_dimension(&exp.ty)) {
                (Some(db), Some(de)) if db.is_dimensionless() => {
                    if !de.is_dimensionless() {
                        return Err(DimError::Mismatch {
                            expected: Dimension::dimensionless(),
                            got: de,
                            context: "exponent".to_string(),
                            span: exp.span,
                        });
                    }
                    Ty::Concrete(Dimension::dimensionless())
                }
                (Some(db), Some(_)) => {
                    let n = expr_as_integer(&exp).ok_or(DimError::NonIntegerPower(exp.span))?;
                    Ty::Concrete(db.pow(n))
                }
                _ => Ty::Unresolved,
            };
            Ok(Expr::spanned_typed(
                ExprKind::Pow(Box::new(base), Box::new(exp)),
                span,
                ty,
            ))
        }
        ExprKind::Fn(kind, arg) => {
            let arg = elaborate_expr(arg, env)?;
            let ty = match ty_dimension(&arg.ty) {
                Some(dim) if dim.is_dimensionless() => Ty::Concrete(Dimension::dimensionless()),
                Some(dim) => {
                    return Err(DimError::NonDimensionlessFnArg {
                        func: format!("{:?}", kind).to_lowercase(),
                        dim,
                        span: arg.span,
                    });
                }
                None => Ty::Unresolved,
            };
            Ok(Expr::spanned_typed(
                ExprKind::Fn(kind.clone(), Box::new(arg)),
                span,
                ty,
            ))
        }
        ExprKind::FnN(kind, args) => {
            let args: Vec<Expr> = args
                .iter()
                .map(|arg| elaborate_expr(arg, env))
                .collect::<Result<_, _>>()?;
            let mut unresolved = false;
            for arg in &args {
                match ty_dimension(&arg.ty) {
                    Some(dim) if dim.is_dimensionless() => {}
                    Some(dim) => {
                        return Err(DimError::NonDimensionlessFnArg {
                            func: format!("{:?}", kind).to_lowercase(),
                            dim,
                            span: arg.span,
                        });
                    }
                    None => unresolved = true,
                }
            }
            let ty = if unresolved {
                Ty::Unresolved
            } else {
                Ty::Concrete(Dimension::dimensionless())
            };
            Ok(Expr::spanned_typed(
                ExprKind::FnN(kind.clone(), args),
                span,
                ty,
            ))
        }
    }
}

/// Check that two sides of a claim have the same dimension.
pub fn check_claim_dim(lhs: &Expr, rhs: &Expr, env: &DimEnv) -> DimOutcome {
    let dl = match check_dim(lhs, env) {
        Ok(d) => d,
        Err(DimError::UndeclaredVar(v, _)) => {
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
        Err(DimError::UndeclaredVar(v, _)) => {
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
    match &expr.kind {
        ExprKind::Var { name, dim, .. } => {
            if dim.is_none() && !env.contains_key(name) {
                out.push(name.clone());
            }
        }
        ExprKind::Add(a, b) | ExprKind::Mul(a, b) | ExprKind::Pow(a, b) => {
            collect_undeclared_inner(a, env, out);
            collect_undeclared_inner(b, env, out);
        }
        ExprKind::Neg(inner) | ExprKind::Inv(inner) | ExprKind::Fn(_, inner) => {
            collect_undeclared_inner(inner, env, out);
        }
        ExprKind::FnN(_, args) => {
            for arg in args {
                collect_undeclared_inner(arg, env, out);
            }
        }
        ExprKind::Rational(_) | ExprKind::FracPi(_) | ExprKind::Named(_) => {}
        ExprKind::Quantity(inner, _) => {
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
    match &expr.kind {
        ExprKind::Quantity(_, _) => true,
        ExprKind::Var { dim: Some(_), .. } => true,
        ExprKind::Add(a, b) | ExprKind::Mul(a, b) | ExprKind::Pow(a, b) => {
            has_type_info(a) || has_type_info(b)
        }
        ExprKind::Neg(inner) | ExprKind::Inv(inner) | ExprKind::Fn(_, inner) => {
            has_type_info(inner)
        }
        ExprKind::FnN(_, args) => args.iter().any(has_type_info),
        _ => false,
    }
}

fn expr_as_integer(expr: &Expr) -> Option<i32> {
    match &expr.kind {
        ExprKind::Rational(r) => {
            if r.den() == 1 {
                Some(r.num() as i32)
            } else {
                None
            }
        }
        ExprKind::Neg(inner) => expr_as_integer(inner).map(|n| -n),
        _ => None,
    }
}

fn respan_tree(expr: &Expr, span: Span) -> Expr {
    let ty = expr.ty.clone();
    let kind = match &expr.kind {
        ExprKind::Rational(r) => ExprKind::Rational(*r),
        ExprKind::FracPi(r) => ExprKind::FracPi(*r),
        ExprKind::Named(n) => ExprKind::Named(*n),
        ExprKind::Var { name, indices, dim } => ExprKind::Var {
            name: name.clone(),
            indices: indices.clone(),
            dim: dim.clone(),
        },
        ExprKind::Add(a, b) => ExprKind::Add(
            Box::new(respan_tree(a, span)),
            Box::new(respan_tree(b, span)),
        ),
        ExprKind::Mul(a, b) => ExprKind::Mul(
            Box::new(respan_tree(a, span)),
            Box::new(respan_tree(b, span)),
        ),
        ExprKind::Pow(a, b) => ExprKind::Pow(
            Box::new(respan_tree(a, span)),
            Box::new(respan_tree(b, span)),
        ),
        ExprKind::Neg(inner) => ExprKind::Neg(Box::new(respan_tree(inner, span))),
        ExprKind::Inv(inner) => ExprKind::Inv(Box::new(respan_tree(inner, span))),
        ExprKind::Fn(kind, inner) => ExprKind::Fn(kind.clone(), Box::new(respan_tree(inner, span))),
        ExprKind::FnN(kind, args) => ExprKind::FnN(
            kind.clone(),
            args.iter().map(|arg| respan_tree(arg, span)).collect(),
        ),
        ExprKind::Quantity(inner, unit) => {
            ExprKind::Quantity(Box::new(respan_tree(inner, span)), unit.clone())
        }
    };
    Expr::spanned_typed(kind, span, ty)
}

/// Convert an Expr into a Pattern, turning free variables into wildcards.
fn expr_to_pattern(expr: &Expr, wildcards: &HashSet<String>) -> Pattern {
    match &expr.kind {
        ExprKind::Var { name, .. } => {
            if wildcards.contains(name) {
                Pattern::Wildcard(name.clone())
            } else {
                rule::p_var(name, vec![])
            }
        }
        ExprKind::Rational(r) => Pattern::Rational(*r),
        ExprKind::FracPi(r) => Pattern::FracPi(*r),
        ExprKind::Named(n) => Pattern::Named(*n),
        ExprKind::Add(a, b) => Pattern::Add(
            Box::new(expr_to_pattern(a, wildcards)),
            Box::new(expr_to_pattern(b, wildcards)),
        ),
        ExprKind::Mul(a, b) => Pattern::Mul(
            Box::new(expr_to_pattern(a, wildcards)),
            Box::new(expr_to_pattern(b, wildcards)),
        ),
        ExprKind::Pow(a, b) => Pattern::Pow(
            Box::new(expr_to_pattern(a, wildcards)),
            Box::new(expr_to_pattern(b, wildcards)),
        ),
        ExprKind::Neg(inner) => Pattern::Neg(Box::new(expr_to_pattern(inner, wildcards))),
        ExprKind::Inv(inner) => Pattern::Inv(Box::new(expr_to_pattern(inner, wildcards))),
        ExprKind::Fn(kind, inner) => {
            Pattern::Fn(kind.clone(), Box::new(expr_to_pattern(inner, wildcards)))
        }
        ExprKind::FnN(kind, args) => Pattern::FnN(
            kind.clone(),
            args.iter().map(|a| expr_to_pattern(a, wildcards)).collect(),
        ),
        ExprKind::Quantity(inner, unit) => {
            Pattern::Quantity(Box::new(expr_to_pattern(inner, wildcards)), unit.clone())
        }
    }
}

/// Substitute all constants in an expression.
/// Replaces `Var { name, .. }` nodes whose name appears in `consts` with the constant value.
pub fn substitute_consts(expr: &Expr, consts: &HashMap<String, Expr>) -> Expr {
    if consts.is_empty() {
        return expr.clone();
    }
    let span = expr.span;
    match &expr.kind {
        ExprKind::Var { name, .. } => {
            if let Some(value) = consts.get(name) {
                // Preserve the call-site span recursively so nested errors
                // point at the user's input, not the const definition site.
                respan_tree(value, span)
            } else {
                expr.clone()
            }
        }
        ExprKind::Add(a, b) => Expr::spanned(
            ExprKind::Add(
                Box::new(substitute_consts(a, consts)),
                Box::new(substitute_consts(b, consts)),
            ),
            span,
        ),
        ExprKind::Mul(a, b) => Expr::spanned(
            ExprKind::Mul(
                Box::new(substitute_consts(a, consts)),
                Box::new(substitute_consts(b, consts)),
            ),
            span,
        ),
        ExprKind::Pow(a, b) => Expr::spanned(
            ExprKind::Pow(
                Box::new(substitute_consts(a, consts)),
                Box::new(substitute_consts(b, consts)),
            ),
            span,
        ),
        ExprKind::Neg(inner) => Expr::spanned(
            ExprKind::Neg(Box::new(substitute_consts(inner, consts))),
            span,
        ),
        ExprKind::Inv(inner) => Expr::spanned(
            ExprKind::Inv(Box::new(substitute_consts(inner, consts))),
            span,
        ),
        ExprKind::Fn(kind, inner) => Expr::spanned(
            ExprKind::Fn(kind.clone(), Box::new(substitute_consts(inner, consts))),
            span,
        ),
        ExprKind::FnN(kind, args) => Expr::spanned(
            ExprKind::FnN(
                kind.clone(),
                args.iter().map(|a| substitute_consts(a, consts)).collect(),
            ),
            span,
        ),
        ExprKind::Quantity(inner, unit) => Expr::spanned(
            ExprKind::Quantity(Box::new(substitute_consts(inner, consts)), unit.clone()),
            span,
        ),
        ExprKind::Rational(_) | ExprKind::FracPi(_) | ExprKind::Named(_) => expr.clone(),
    }
}

/// Expand user-defined function calls in an expression.
/// Replaces `Fn(Custom(name), arg)` / `FnN(Custom(name), args)` with the function body,
/// substituting parameters with the provided arguments.
fn expand_funcs(expr: &Expr, funcs: &HashMap<String, FuncDef>) -> Expr {
    if funcs.is_empty() {
        return expr.clone();
    }
    let span = expr.span;
    match &expr.kind {
        ExprKind::Fn(crate::expr::FnKind::Custom(name), arg) => {
            if let Some(def) = funcs.get(name) {
                let expanded_arg = expand_funcs(arg, funcs);
                let mut bindings = HashMap::new();
                if let Some(param) = def.params.first() {
                    bindings.insert(param.clone(), expanded_arg);
                }
                let result = substitute_consts(&def.body, &bindings);
                let expanded = expand_funcs(&result, funcs);
                respan_tree(&expanded, span)
            } else {
                Expr::spanned(
                    ExprKind::Fn(
                        crate::expr::FnKind::Custom(name.clone()),
                        Box::new(expand_funcs(arg, funcs)),
                    ),
                    span,
                )
            }
        }
        ExprKind::FnN(crate::expr::FnKind::Custom(name), args) => {
            if let Some(def) = funcs.get(name) {
                let expanded_args: Vec<Expr> =
                    args.iter().map(|a| expand_funcs(a, funcs)).collect();
                let mut bindings = HashMap::new();
                for (param, arg) in def.params.iter().zip(expanded_args) {
                    bindings.insert(param.clone(), arg);
                }
                let result = substitute_consts(&def.body, &bindings);
                let expanded = expand_funcs(&result, funcs);
                respan_tree(&expanded, span)
            } else {
                Expr::spanned(
                    ExprKind::FnN(
                        crate::expr::FnKind::Custom(name.clone()),
                        args.iter().map(|a| expand_funcs(a, funcs)).collect(),
                    ),
                    span,
                )
            }
        }
        ExprKind::Add(a, b) => Expr::spanned(
            ExprKind::Add(
                Box::new(expand_funcs(a, funcs)),
                Box::new(expand_funcs(b, funcs)),
            ),
            span,
        ),
        ExprKind::Mul(a, b) => Expr::spanned(
            ExprKind::Mul(
                Box::new(expand_funcs(a, funcs)),
                Box::new(expand_funcs(b, funcs)),
            ),
            span,
        ),
        ExprKind::Pow(a, b) => Expr::spanned(
            ExprKind::Pow(
                Box::new(expand_funcs(a, funcs)),
                Box::new(expand_funcs(b, funcs)),
            ),
            span,
        ),
        ExprKind::Neg(inner) => {
            Expr::spanned(ExprKind::Neg(Box::new(expand_funcs(inner, funcs))), span)
        }
        ExprKind::Inv(inner) => {
            Expr::spanned(ExprKind::Inv(Box::new(expand_funcs(inner, funcs))), span)
        }
        ExprKind::Fn(kind, inner) => Expr::spanned(
            ExprKind::Fn(kind.clone(), Box::new(expand_funcs(inner, funcs))),
            span,
        ),
        ExprKind::FnN(kind, args) => Expr::spanned(
            ExprKind::FnN(
                kind.clone(),
                args.iter().map(|a| expand_funcs(a, funcs)).collect(),
            ),
            span,
        ),
        ExprKind::Quantity(inner, unit) => Expr::spanned(
            ExprKind::Quantity(Box::new(expand_funcs(inner, funcs)), unit.clone()),
            span,
        ),
        ExprKind::Rational(_) | ExprKind::FracPi(_) | ExprKind::Named(_) | ExprKind::Var { .. } => {
            expr.clone()
        }
    }
}

fn try_rule_produces_inner(
    from: &Expr,
    rule: &crate::rule::Rule,
    to: &Expr,
    ctx: &Context,
) -> bool {
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

    match &from.kind {
        ExprKind::Add(a, b) | ExprKind::Mul(a, b) | ExprKind::Pow(a, b) => {
            try_rule_produces_inner(a, rule, to, ctx) || try_rule_produces_inner(b, rule, to, ctx)
        }
        ExprKind::Neg(inner) | ExprKind::Inv(inner) | ExprKind::Fn(_, inner) => {
            try_rule_produces_inner(inner, rule, to, ctx)
        }
        ExprKind::FnN(_, args) => args
            .iter()
            .any(|a| try_rule_produces_inner(a, rule, to, ctx)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dim::BaseDim;
    use crate::expr::Ty;
    use crate::parser::parse_expr;
    use crate::rule::Pattern;

    #[test]
    fn check_expr_dim_catches_length_plus_time() {
        let mut ctx = Context::new();
        ctx.declare_var("b", Some(Dimension::single(BaseDim::L, 1)));
        let expr = parse_expr("b + 4 [s]").unwrap();
        let result = ctx.check_expr_dim(&expr);
        assert!(result.is_some());
        assert!(
            result.unwrap().is_err(),
            "adding length to time should fail"
        );
    }

    #[test]
    fn check_expr_dim_ok_same_dimension() {
        let mut ctx = Context::new();
        ctx.declare_var("b", Some(Dimension::single(BaseDim::L, 1)));
        let expr = parse_expr("b + 4 [m]").unwrap();
        let result = ctx.check_expr_dim(&expr);
        assert!(result.is_some());
        assert!(
            result.unwrap().is_ok(),
            "adding length to meters should pass"
        );
    }

    #[test]
    fn check_expr_dim_typed_plus_dimensionless_is_error() {
        // x defaults to [1], 4 [s] has type [T] → error
        let ctx = Context::new();
        let expr = parse_expr("x + 4 [s]").unwrap();
        let result = ctx.check_expr_dim(&expr);
        assert!(
            result.is_some(),
            "should not suppress when expression has units"
        );
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
            Err(DimError::UndeclaredVar(..)) => {
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
        assert_eq!(
            ctx.infer_type(&expr),
            Some(Dimension::single(BaseDim::L, 1))
        );
        let expr = parse_expr("a * b").unwrap();
        assert_eq!(
            ctx.infer_type(&expr),
            Some(Dimension::single(BaseDim::L, 2))
        );
    }

    #[test]
    fn infer_type_with_units_no_vars() {
        // No :var declarations — should still infer type from explicit units
        let ctx = Context::new();
        let expr = parse_expr("3 [m]").unwrap();
        assert_eq!(
            ctx.infer_type(&expr),
            Some(Dimension::single(BaseDim::L, 1))
        );
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
    fn infer_ty_dimensionless_returns_concrete_one() {
        let ctx = Context::new();
        let expr = parse_expr("4.5").unwrap();
        assert_eq!(
            ctx.infer_ty(&expr),
            Some(Ty::Concrete(Dimension::dimensionless()))
        );
    }

    #[test]
    fn infer_ty_undeclared_var_returns_dimensionless() {
        let ctx = Context::new();
        let expr = parse_expr("x + 3").unwrap();
        assert_eq!(
            ctx.infer_ty(&expr),
            Some(Ty::Concrete(Dimension::dimensionless()))
        );
    }

    #[test]
    fn infer_ty_bare_undeclared_var_returns_dimensionless() {
        let ctx = Context::new();
        let expr = parse_expr("x").unwrap();
        assert_eq!(
            ctx.infer_ty(&expr),
            Some(Ty::Concrete(Dimension::dimensionless()))
        );
    }

    #[test]
    fn elaborate_expr_marks_dimensionless_literals_as_concrete() {
        let ctx = Context::new();
        let expr = ctx.elaborate_expr(&parse_expr("4.5").unwrap()).unwrap();
        assert_eq!(expr.ty, Ty::Concrete(Dimension::dimensionless()));
    }

    #[test]
    fn elaborate_expr_marks_quantities_with_unit_dimension() {
        let ctx = Context::new();
        let expr = ctx.elaborate_expr(&parse_expr("4.5 [m]").unwrap()).unwrap();
        assert_eq!(expr.ty, Ty::Concrete(Dimension::single(BaseDim::L, 1)));
    }

    #[test]
    fn elaborate_expr_marks_undeclared_vars_dimensionless() {
        let ctx = Context::new();
        let expr = ctx.elaborate_expr(&parse_expr("x + 1").unwrap()).unwrap();
        match &expr.kind {
            ExprKind::Add(lhs, rhs) => {
                assert_eq!(lhs.ty, Ty::Concrete(Dimension::dimensionless()));
                assert_eq!(rhs.ty, Ty::Concrete(Dimension::dimensionless()));
                assert_eq!(expr.ty, Ty::Concrete(Dimension::dimensionless()));
            }
            other => panic!("expected addition, got {:?}", other),
        }
    }

    #[test]
    fn elaborate_expr_uses_declared_dimensions_for_vars() {
        let mut ctx = Context::new();
        ctx.declare_var("x", Some(Dimension::single(BaseDim::L, 1)));
        let expr = ctx
            .elaborate_expr(&parse_expr("x + 4 [m]").unwrap())
            .unwrap();
        match &expr.kind {
            ExprKind::Add(lhs, rhs) => {
                assert_eq!(lhs.ty, Ty::Concrete(Dimension::single(BaseDim::L, 1)));
                assert_eq!(rhs.ty, Ty::Concrete(Dimension::single(BaseDim::L, 1)));
                assert_eq!(expr.ty, Ty::Concrete(Dimension::single(BaseDim::L, 1)));
            }
            other => panic!("expected addition, got {:?}", other),
        }
    }

    #[test]
    fn check_expr_dim_const_with_units_no_vars() {
        // No :var declarations, but const has units — should still catch mismatch
        let mut ctx = Context::new();
        ctx.declare_const("c", parse_expr("3 [m/s]").unwrap());
        let expr = parse_expr("c + 1").unwrap();
        let result = ctx.check_expr_dim(&expr);
        assert!(result.is_some(), "should check dims even without :var");
        assert!(
            result.unwrap().is_err(),
            "adding [m/s] to dimensionless should fail"
        );
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
    fn check_expr_dim_undeclared_var_no_vars_is_dimensionless() {
        // No declarations, bare variables are explicitly dimensionless
        let ctx = Context::new();
        let expr = parse_expr("x + 3").unwrap();
        match ctx.check_expr_dim(&expr) {
            Some(Ok(dim)) => assert_eq!(dim, Dimension::dimensionless()),
            other => panic!("expected dimensionless result, got {:?}", other),
        }
    }

    #[test]
    fn check_expr_dim_undeclared_var_with_units_is_error() {
        // No :var declarations, but expression mixes units with an implicit [1] var → error
        let ctx = Context::new();
        let expr = parse_expr("1 [m] + 2 [km] + x").unwrap();
        let result = ctx.check_expr_dim(&expr);
        assert!(
            result.is_some(),
            "should not suppress when expression has units"
        );
        assert!(result.unwrap().is_err());
    }

    #[test]
    fn dimensionless_var_in_typed_addition_reports_mismatch() {
        // An implicit [1] var in typed addition should produce a dimension mismatch
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
            "dimensionless symbol should show as [1], got: {}",
            msg
        );
    }

    #[test]
    fn check_expr_dim_inline_dim_with_undeclared_var_is_error() {
        // x has inline dim [L], y has no type → error
        let ctx = Context::new();
        let expr = parse_expr("x [L] + y").unwrap();
        let result = ctx.check_expr_dim(&expr);
        assert!(
            result.is_some(),
            "should not suppress when expression has inline dims"
        );
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
        let expr = Expr::new(ExprKind::Fn(
            FnKind::Custom("f".to_string()),
            Box::new(parse_expr("3").unwrap()),
        ));
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
        let expr = Expr::new(ExprKind::FnN(
            crate::expr::FnKind::Custom("g".to_string()),
            vec![parse_expr("2").unwrap(), parse_expr("3").unwrap()],
        ));
        let expanded = ctx.apply_consts(&expr);
        assert_eq!(eval_f64(&expanded, &HashMap::new()), Some(5.0));
    }

    #[test]
    fn expand_unknown_custom_fn_unchanged() {
        use crate::expr::FnKind;
        let ctx = Context::new();
        let expr = Expr::new(ExprKind::Fn(
            FnKind::Custom("f".to_string()),
            Box::new(parse_expr("x").unwrap()),
        ));
        let expanded = ctx.apply_consts(&expr);
        assert_eq!(expanded, expr);
    }

    #[test]
    fn simplify_preserves_type_through_rule_application() {
        let mut ctx = Context::new();
        ctx.declare_var("x", Some(Dimension::single(BaseDim::L, 1)));
        let result = ctx.simplify(&parse_expr("x * 1").unwrap());
        assert_eq!(result.ty, Ty::Concrete(Dimension::single(BaseDim::L, 1)));
    }

    #[test]
    fn add_claim_as_rule_preserves_quantity_patterns() {
        let mut ctx = Context::new();
        let lhs = parse_expr("1 [km]").unwrap();
        let rhs = parse_expr("1000 [m]").unwrap();

        ctx.add_claim_as_rule("unit_conv", &lhs, &rhs);

        let rule = ctx.rules.find_rule("claim:unit_conv").unwrap();
        match (&rule.lhs, &rule.rhs) {
            (Pattern::Quantity(_, lhs_unit), Pattern::Quantity(_, rhs_unit)) => {
                assert_eq!(lhs_unit.display, "km");
                assert_eq!(rhs_unit.display, "m");
            }
            _ => panic!("expected quantity patterns"),
        }
    }

    // --- DimError Display ---

    #[test]
    fn dim_error_display_undeclared_var() {
        let err = DimError::UndeclaredVar("x".to_string(), Span::default());
        assert!(err.to_string().contains("x"));
        assert!(err.to_string().contains("typeless"));
    }

    #[test]
    fn dim_error_display_mismatch() {
        let err = DimError::Mismatch {
            expected: Dimension::single(BaseDim::L, 1),
            got: Dimension::single(BaseDim::T, 1),
            context: "addition".to_string(),
            span: Span::default(),
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
            span: Span::default(),
        };
        let s = err.to_string();
        assert!(s.contains("sin"));
        assert!(s.contains("dimensionless"));
    }

    #[test]
    fn dim_error_display_non_integer_power() {
        let err = DimError::NonIntegerPower(Span::default());
        assert!(err.to_string().contains("non-integer power"));
    }

    #[test]
    fn dim_error_display_typeless_as_mismatch() {
        // Dimensionless values show as [1] in mismatch errors
        let err = DimError::Mismatch {
            expected: Dimension::single(BaseDim::L, 1),
            got: Dimension::dimensionless(),
            context: "addition".to_string(),
            span: Span::default(),
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
            error: DimError::NonIntegerPower(Span::default()),
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
        assert!(matches!(result, Err(DimError::NonIntegerPower(_))));
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
        let expr = Expr::new(ExprKind::FnN(
            crate::expr::FnKind::Min,
            vec![parse_expr("x").unwrap(), parse_expr("1").unwrap()],
        ));
        let result = check_dim(&expr, &env);
        assert!(matches!(
            result,
            Err(DimError::NonDimensionlessFnArg { .. })
        ));
    }

    #[test]
    fn check_dim_uses_elaborated_ty_without_env_lookup() {
        let mut ctx = Context::new();
        ctx.declare_var("x", Some(Dimension::single(BaseDim::L, 1)));
        let expr = ctx.elaborate_expr(&parse_expr("x * x").unwrap()).unwrap();
        let dim = check_dim(&expr, &DimEnv::new()).unwrap();
        assert_eq!(dim, Dimension::single(BaseDim::L, 2));
    }

    // --- check_claim_dim paths ---

    #[test]
    fn check_claim_dim_rhs_dimensionless_mismatches() {
        let mut env = DimEnv::new();
        env.insert("a".into(), Dimension::single(BaseDim::L, 1));
        // lhs declared as [L], rhs defaults to [1]
        let lhs = parse_expr("a").unwrap();
        let rhs = parse_expr("b").unwrap();
        let result = check_claim_dim(&lhs, &rhs, &env);
        assert!(matches!(
            result,
            DimOutcome::LhsRhsMismatch {
                lhs,
                rhs
            } if lhs == Dimension::single(BaseDim::L, 1) && rhs == Dimension::dimensionless()
        ));
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

    #[test]
    fn exprs_equivalent_substitutes_consts() {
        let mut ctx = Context::new();
        ctx.declare_const("a", parse_expr("3").unwrap());

        let a = parse_expr("a * (x + 1)").unwrap();
        let b = parse_expr("3 * x + 3").unwrap();
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

    // --- DimError::span ---

    #[test]
    fn dim_error_span_from_mismatch() {
        let mut ctx = Context::new();
        ctx.declare_var("v", Some(Dimension::single(BaseDim::L, 1)));
        let source = "v + 4 [s]";
        let expr = parse_expr(source).unwrap();
        let err = ctx.check_expr_dim(&expr).unwrap().unwrap_err();
        let span = err.span();
        // The span should point at the "4 [s]" part (the "got" side)
        assert!(span.start > 0, "span should not start at 0");
        assert!(span.end <= source.chars().count());
    }

    // --- format_dim_error ---

    #[test]
    fn format_dim_error_produces_caret_underline() {
        let mut ctx = Context::new();
        ctx.declare_var("v", Some(Dimension::single(BaseDim::L, 1)));
        let source = "v + 4 [s]";
        let expr = parse_expr(source).unwrap();
        let err = ctx.check_expr_dim(&expr).unwrap().unwrap_err();
        let formatted = format_dim_error(&err, source, 2); // 2 for "> " prompt
                                                           // Should contain carets
        assert!(formatted.contains('^'), "should have caret underline");
        // Should contain the error message
        assert!(formatted.contains("dimension mismatch"));
    }

    #[test]
    fn format_dim_error_caret_width_matches_span() {
        let err = DimError::Mismatch {
            expected: Dimension::dimensionless(),
            got: Dimension::single(BaseDim::L, 1),
            context: "test".to_string(),
            span: Span::new(5, 10),
        };
        let source = "hello world test";
        let formatted = format_dim_error(&err, source, 0);
        // Extract the caret line (first line of the formatted output, stripping ANSI)
        let plain: String = formatted.chars().filter(|c| !c.is_control()).collect();
        // Should have 5 carets (span length = 10 - 5)
        assert!(plain.contains("^^^^^"));
    }

    #[test]
    fn format_dim_error_prefix_width_offsets_carets() {
        let err = DimError::UndeclaredVar("x".to_string(), Span::new(0, 1));
        let source = "x + y";
        let formatted = format_dim_error(&err, source, 4); // 4 char prefix
        let lines: Vec<&str> = formatted.lines().collect();
        // First line is caret line — should have 4 spaces before the caret
        let caret_line = lines[0];
        // Strip ANSI escape
        let plain: String = caret_line.replace("\x1b[31m", "").replace("\x1b[0m", "");
        assert!(
            plain.starts_with("    ^"),
            "carets should be offset by prefix_width"
        );
    }

    // --- span provenance through apply_consts ---

    #[test]
    fn error_span_valid_after_const_substitution() {
        // Regression: substitute_consts must preserve spans of compound nodes
        // so that DimError spans remain within the current source bounds.
        let mut ctx = Context::new();
        ctx.declare_var("v", Some(Dimension::single(BaseDim::L, 1)));
        ctx.declare_const("c", parse_expr("3*10^8 [m/s]").unwrap());
        let source = "v + c";
        let expr = parse_expr(source).unwrap();
        let result = ctx.check_expr_dim(&expr);
        assert!(result.is_some());
        if let Some(Err(e)) = result {
            let span = e.span();
            let source_len = source.chars().count();
            assert!(
                span.end <= source_len,
                "error span end {} exceeds source length {} — \
                 apply_consts likely corrupted spans",
                span.end,
                source_len,
            );
            assert!(
                span.start < source_len,
                "error span start {} at or beyond source length {}",
                span.start,
                source_len,
            );
        }
    }

    #[test]
    fn nested_error_span_valid_after_const_substitution() {
        // Regression: nested spans inside a substituted const body must also
        // be rewritten to the call site, not just the root node.
        let mut ctx = Context::new();
        ctx.declare_const("c", parse_expr("1 [m] + 2 [s]").unwrap());
        let source = "c";
        let expr = parse_expr(source).unwrap();
        let result = ctx.check_expr_dim(&expr);
        assert!(result.is_some());
        if let Some(Err(e)) = result {
            let span = e.span();
            let source_len = source.chars().count();
            assert!(
                span.end <= source_len,
                "nested const span end {} exceeds source length {}",
                span.end,
                source_len,
            );
        }
    }

    #[test]
    fn error_span_valid_after_func_expansion() {
        // Regression: expand_funcs must preserve spans of compound nodes.
        let mut ctx = Context::new();
        ctx.declare_var("v", Some(Dimension::single(BaseDim::L, 1)));
        ctx.declare_func("f", vec!["x".to_string()], parse_expr("x").unwrap());
        let source = "v + f(4 [s])";
        let expr = parse_expr(source).unwrap();
        let result = ctx.check_expr_dim(&expr);
        assert!(result.is_some());
        if let Some(Err(e)) = result {
            let span = e.span();
            let source_len = source.chars().count();
            assert!(
                span.end <= source_len,
                "error span end {} exceeds source length {} — \
                 expand_funcs likely corrupted spans",
                span.end,
                source_len,
            );
        }
    }

    #[test]
    fn nested_error_span_valid_after_func_expansion() {
        // Regression: nested spans inside an expanded function body must be
        // rewritten to the call site, not left at definition-site offsets.
        let mut ctx = Context::new();
        ctx.declare_func(
            "foo",
            vec!["x".to_string()],
            parse_expr("1 [m] + x").unwrap(),
        );
        let source = "foo(2 [s])";
        let expr = parse_expr(source).unwrap();
        let result = ctx.check_expr_dim(&expr);
        assert!(result.is_some());
        if let Some(Err(e)) = result {
            let span = e.span();
            let source_len = source.chars().count();
            assert!(
                span.end <= source_len,
                "nested function span end {} exceeds source length {}",
                span.end,
                source_len,
            );
        }
    }
}
