use crate::dim::Dimension;
use crate::rational::Rational;
use crate::unit::Unit;

/// Source location span for error reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Span { start, end }
    }
}

/// Expression node with source location span.
#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
    pub ty: Ty,
}

impl Expr {
    pub fn new(kind: ExprKind) -> Self {
        Expr {
            kind,
            span: Span::default(),
            ty: Ty::Unresolved,
        }
    }

    pub fn spanned(kind: ExprKind, span: Span) -> Self {
        Expr {
            kind,
            span,
            ty: Ty::Unresolved,
        }
    }

    pub fn typed(kind: ExprKind, ty: Ty) -> Self {
        Expr {
            kind,
            span: Span::default(),
            ty,
        }
    }

    pub fn spanned_typed(kind: ExprKind, span: Span, ty: Ty) -> Self {
        Expr { kind, span, ty }
    }

    pub fn derived(kind: ExprKind) -> Self {
        let ty = infer_ty_from_kind(&kind);
        Expr {
            kind,
            span: Span::default(),
            ty,
        }
    }

    pub fn spanned_derived(kind: ExprKind, span: Span) -> Self {
        let ty = infer_ty_from_kind(&kind);
        Expr { kind, span, ty }
    }

}

struct StripConfig {
    clear_ty: bool,
    unwrap_quantity: bool,
}

impl Expr {
    /// Project a typed expression into the explicit untyped token/ML boundary.
    /// Clears all type info (`ty` → Unresolved, `dim` → None) and unwraps Quantity nodes.
    pub fn strip_types(&self) -> Self {
        self.strip(&StripConfig {
            clear_ty: true,
            unwrap_quantity: true,
        })
    }

    /// Remove inline dimension annotations from Var nodes, preserving Ty and Quantity.
    /// Used before display so the combined type suffix is shown instead of per-variable dims.
    pub fn strip_dim_annotations(&self) -> Self {
        self.strip(&StripConfig {
            clear_ty: false,
            unwrap_quantity: false,
        })
    }

    fn strip(&self, cfg: &StripConfig) -> Self {
        let kind = match &self.kind {
            ExprKind::Var { name, indices, .. } => ExprKind::Var {
                name: name.clone(),
                indices: indices.clone(),
                dim: None,
            },
            ExprKind::Add(a, b) => {
                ExprKind::Add(Box::new(a.strip(cfg)), Box::new(b.strip(cfg)))
            }
            ExprKind::Mul(a, b) => {
                ExprKind::Mul(Box::new(a.strip(cfg)), Box::new(b.strip(cfg)))
            }
            ExprKind::Pow(a, b) => {
                ExprKind::Pow(Box::new(a.strip(cfg)), Box::new(b.strip(cfg)))
            }
            ExprKind::Neg(inner) => ExprKind::Neg(Box::new(inner.strip(cfg))),
            ExprKind::Inv(inner) => ExprKind::Inv(Box::new(inner.strip(cfg))),
            ExprKind::Fn(kind, inner) => {
                ExprKind::Fn(kind.clone(), Box::new(inner.strip(cfg)))
            }
            ExprKind::FnN(kind, args) => {
                ExprKind::FnN(kind.clone(), args.iter().map(|a| a.strip(cfg)).collect())
            }
            ExprKind::Quantity(inner, unit) => {
                if cfg.unwrap_quantity {
                    let mut stripped = inner.strip(cfg);
                    stripped.span = self.span;
                    return stripped;
                }
                ExprKind::Quantity(Box::new(inner.strip(cfg)), unit.clone())
            }
            other => other.clone(),
        };
        let ty = if cfg.clear_ty {
            Ty::Unresolved
        } else {
            self.ty.clone()
        };
        Expr {
            kind,
            span: self.span,
            ty,
        }
    }
}

impl PartialEq for Expr {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Ty {
    Concrete(Dimension),
    #[default]
    Unresolved,
}

pub fn infer_ty_from_kind(kind: &ExprKind) -> Ty {
    match kind {
        ExprKind::Rational(_) | ExprKind::FracPi(_) | ExprKind::Named(_) => {
            Ty::Concrete(Dimension::dimensionless())
        }
        ExprKind::Var { dim: Some(dim), .. } => Ty::Concrete(dim.clone()),
        ExprKind::Var { dim: None, .. } => Ty::Concrete(Dimension::dimensionless()),
        ExprKind::Add(a, b) => match (&a.ty, &b.ty) {
            (Ty::Concrete(da), Ty::Concrete(db)) if da == db => Ty::Concrete(da.clone()),
            _ => Ty::Unresolved,
        },
        ExprKind::Mul(a, b) => match (&a.ty, &b.ty) {
            (Ty::Concrete(da), Ty::Concrete(db)) => Ty::Concrete(da.mul(db)),
            _ => Ty::Unresolved,
        },
        ExprKind::Neg(inner) => inner.ty.clone(),
        ExprKind::Inv(inner) => match &inner.ty {
            Ty::Concrete(dim) => Ty::Concrete(dim.inv()),
            Ty::Unresolved => Ty::Unresolved,
        },
        ExprKind::Pow(base, exp) => match (&base.ty, &exp.ty) {
            (Ty::Concrete(dim), Ty::Concrete(exp_dim)) if dim.is_dimensionless() && exp_dim.is_dimensionless() => {
                Ty::Concrete(Dimension::dimensionless())
            }
            (Ty::Concrete(dim), Ty::Concrete(_)) => match expr_as_integer(exp) {
                Some(n) => Ty::Concrete(dim.pow(n)),
                None => Ty::Unresolved,
            },
            _ => Ty::Unresolved,
        },
        ExprKind::Fn(_, inner) => match &inner.ty {
            Ty::Concrete(dim) if dim.is_dimensionless() => Ty::Concrete(Dimension::dimensionless()),
            _ => Ty::Unresolved,
        },
        ExprKind::FnN(_, args) => {
            if args
                .iter()
                .all(|arg| matches!(&arg.ty, Ty::Concrete(dim) if dim.is_dimensionless()))
            {
                Ty::Concrete(Dimension::dimensionless())
            } else {
                Ty::Unresolved
            }
        }
        ExprKind::Quantity(_, unit) => Ty::Concrete(unit.dimension.clone()),
    }
}

fn expr_as_integer(expr: &Expr) -> Option<i32> {
    match &expr.kind {
        ExprKind::Rational(r) if r.den() == 1 => i32::try_from(r.num()).ok(),
        ExprKind::Neg(inner) => expr_as_integer(inner).and_then(|n| n.checked_neg()),
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    // Atoms
    Rational(Rational), // Exact rational number
    FracPi(Rational),   // Rational multiple of π (value = r * π)
    Named(NamedConst),  // Named mathematical constant (e, √2, etc.)
    Var {
        name: String,
        indices: Vec<Index>,
        dim: Option<Dimension>,
    },

    // Arithmetic
    Add(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Neg(Box<Expr>),
    Inv(Box<Expr>), // multiplicative inverse (1/x)
    Pow(Box<Expr>, Box<Expr>),

    // Functions
    Fn(FnKind, Box<Expr>),
    FnN(FnKind, Vec<Expr>),

    // Quantity: numeric expression with unit annotation
    Quantity(Box<Expr>, Unit),
}

impl PartialEq for ExprKind {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ExprKind::Named(a), ExprKind::Named(b)) => a == b,
            (
                ExprKind::Var {
                    name: n1,
                    indices: i1,
                    ..
                },
                ExprKind::Var {
                    name: n2,
                    indices: i2,
                    ..
                },
            ) => n1 == n2 && i1 == i2,
            (ExprKind::Add(a1, b1), ExprKind::Add(a2, b2))
            | (ExprKind::Mul(a1, b1), ExprKind::Mul(a2, b2))
            | (ExprKind::Pow(a1, b1), ExprKind::Pow(a2, b2)) => a1 == a2 && b1 == b2,
            (ExprKind::Neg(a), ExprKind::Neg(b)) | (ExprKind::Inv(a), ExprKind::Inv(b)) => a == b,
            (ExprKind::Fn(k1, a), ExprKind::Fn(k2, b)) => k1 == k2 && a == b,
            (ExprKind::FnN(k1, a), ExprKind::FnN(k2, b)) => k1 == k2 && a == b,
            (ExprKind::Rational(a), ExprKind::Rational(b)) => a == b,
            (ExprKind::FracPi(a), ExprKind::FracPi(b)) => a == b,
            (ExprKind::Quantity(a1, u1), ExprKind::Quantity(a2, u2)) => a1 == a2 && u1 == u2,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Index {
    pub name: String,
    pub position: IndexPosition,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IndexPosition {
    Upper, // contravariant
    Lower, // covariant
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FnKind {
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    Sign,
    Sinh,
    Cosh,
    Tanh,
    Floor,
    Ceil,
    Round,
    Min,
    Max,
    Clamp,
    Exp,
    Ln,
    /// User-defined function (from `:func` declarations).
    Custom(String),
}

/// Named mathematical constants with exact symbolic representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NamedConst {
    E,            // e (Euler's number)
    Sqrt2,        // √2
    Sqrt3,        // √3
    Frac1Sqrt2,   // √2/2 = 1/√2
    FracSqrt3By2, // √3/2
}

impl NamedConst {
    /// Get the numerical value of this constant.
    pub fn value(&self) -> f64 {
        use std::f64::consts::*;
        match self {
            NamedConst::E => E,
            NamedConst::Sqrt2 => SQRT_2,
            NamedConst::Sqrt3 => 3.0_f64.sqrt(),
            NamedConst::Frac1Sqrt2 => FRAC_1_SQRT_2,
            NamedConst::FracSqrt3By2 => 3.0_f64.sqrt() / 2.0,
        }
    }

    /// Try to match a numerical value to a named constant (within epsilon).
    pub fn from_value(v: f64) -> Option<NamedConst> {
        use std::f64::consts::*;
        const EPS: f64 = 1e-12;

        let sqrt_3 = 3.0_f64.sqrt();
        let candidates = [
            (E, NamedConst::E),
            (SQRT_2, NamedConst::Sqrt2),
            (sqrt_3, NamedConst::Sqrt3),
            (FRAC_1_SQRT_2, NamedConst::Frac1Sqrt2),
            (sqrt_3 / 2.0, NamedConst::FracSqrt3By2),
        ];

        for (val, nc) in candidates {
            if (v - val).abs() < EPS {
                return Some(nc);
            }
        }
        None
    }
}

impl Expr {
    pub fn complexity(&self) -> usize {
        match &self.kind {
            ExprKind::Rational(_)
            | ExprKind::Named(_)
            | ExprKind::FracPi(_)
            | ExprKind::Var { .. } => 1,
            ExprKind::Quantity(inner, _) => 1 + inner.complexity(),
            ExprKind::Neg(a) | ExprKind::Inv(a) | ExprKind::Fn(_, a) => 1 + a.complexity(),
            ExprKind::FnN(_, args) => 1 + args.iter().map(|a| a.complexity()).sum::<usize>(),
            ExprKind::Add(a, b) | ExprKind::Mul(a, b) | ExprKind::Pow(a, b) => {
                1 + a.complexity() + b.complexity()
            }
        }
    }

    /// Collect all unit display names from Quantity nodes in this expression.
    pub fn collect_units(&self) -> Vec<String> {
        let mut units = Vec::new();
        self.walk(&mut |e| {
            if let ExprKind::Quantity(_, unit) = &e.kind {
                units.push(unit.display.clone());
            }
        });
        units.sort();
        units.dedup();
        units
    }

    /// Return the first Unit found in the expression tree, if any.
    pub fn first_unit(&self) -> Option<&Unit> {
        self.find_map(&|e| match &e.kind {
            ExprKind::Quantity(_, unit) => Some(unit),
            _ => None,
        })
    }

    pub fn precedence(&self) -> u8 {
        match &self.kind {
            ExprKind::Rational(_)
            | ExprKind::Named(_)
            | ExprKind::FracPi(_)
            | ExprKind::Var { .. }
            | ExprKind::Quantity(_, _)
            | ExprKind::Fn(_, _)
            | ExprKind::FnN(_, _) => 100,
            ExprKind::Pow(_, _) => 80,
            ExprKind::Neg(_) | ExprKind::Inv(_) => 70,
            ExprKind::Mul(_, _) => 60,
            ExprKind::Add(_, _) => 50,
        }
    }

    /// Visit every node in the tree, calling `f` on each.
    pub fn walk(&self, f: &mut impl FnMut(&Expr)) {
        f(self);
        self.walk_children(f);
    }

    /// Visit children (but not self) recursively.
    fn walk_children(&self, f: &mut impl FnMut(&Expr)) {
        match &self.kind {
            ExprKind::Add(a, b) | ExprKind::Mul(a, b) | ExprKind::Pow(a, b) => {
                a.walk(f);
                b.walk(f);
            }
            ExprKind::Neg(inner) | ExprKind::Inv(inner) | ExprKind::Fn(_, inner) => {
                inner.walk(f);
            }
            ExprKind::FnN(_, args) => {
                for arg in args {
                    arg.walk(f);
                }
            }
            ExprKind::Quantity(inner, _) => inner.walk(f),
            _ => {}
        }
    }

    /// Return true if any node in the tree satisfies the predicate.
    pub fn any(&self, predicate: &impl Fn(&Expr) -> bool) -> bool {
        if predicate(self) {
            return true;
        }
        match &self.kind {
            ExprKind::Add(a, b) | ExprKind::Mul(a, b) | ExprKind::Pow(a, b) => {
                a.any(predicate) || b.any(predicate)
            }
            ExprKind::Neg(inner) | ExprKind::Inv(inner) | ExprKind::Fn(_, inner) => {
                inner.any(predicate)
            }
            ExprKind::FnN(_, args) => args.iter().any(|a| a.any(predicate)),
            ExprKind::Quantity(inner, _) => inner.any(predicate),
            _ => false,
        }
    }

    /// Return the first value produced by `f` for any node in the tree.
    pub fn find_map<'a, T>(&'a self, f: &impl Fn(&'a Expr) -> Option<T>) -> Option<T> {
        if let Some(val) = f(self) {
            return Some(val);
        }
        match &self.kind {
            ExprKind::Add(a, b) | ExprKind::Mul(a, b) | ExprKind::Pow(a, b) => {
                a.find_map(f).or_else(|| b.find_map(f))
            }
            ExprKind::Neg(inner) | ExprKind::Inv(inner) | ExprKind::Fn(_, inner) => {
                inner.find_map(f)
            }
            ExprKind::FnN(_, args) => args.iter().find_map(|a| a.find_map(f)),
            ExprKind::Quantity(inner, _) => inner.find_map(f),
            _ => None,
        }
    }

    /// True if this expression represents a sqrt exponent (1/2).
    pub fn is_sqrt_exp(&self) -> bool {
        match &self.kind {
            ExprKind::Rational(r) => *r == Rational::new(1, 2),
            ExprKind::Inv(inner) => {
                matches!(&inner.kind, ExprKind::Rational(r) if *r == Rational::TWO)
            }
            _ => false,
        }
    }
}

/// If `Mul(left, right)` represents `ln(arg) * 1/ln(base)` (i.e. log_base(arg)),
/// return `(base, arg)`.
pub fn match_log_base<'a>(left: &'a Expr, right: &'a Expr) -> Option<(&'a Expr, &'a Expr)> {
    match (&left.kind, &right.kind) {
        (ExprKind::Fn(FnKind::Ln, arg), ExprKind::Inv(inner)) => match &inner.kind {
            ExprKind::Fn(FnKind::Ln, base) => Some((base.as_ref(), arg.as_ref())),
            _ => None,
        },
        (ExprKind::Inv(inner), ExprKind::Fn(FnKind::Ln, arg)) => match &inner.kind {
            ExprKind::Fn(FnKind::Ln, base) => Some((base.as_ref(), arg.as_ref())),
            _ => None,
        },
        _ => None,
    }
}

pub fn scalar(name: &str) -> Expr {
    Expr::new(ExprKind::Var {
        name: name.to_string(),
        indices: vec![],
        dim: None,
    })
}

pub fn scalar_dim(name: &str, dim: Dimension) -> Expr {
    Expr::new(ExprKind::Var {
        name: name.to_string(),
        indices: vec![],
        dim: Some(dim),
    })
}

pub fn tensor(name: &str, indices: Vec<Index>) -> Expr {
    Expr::new(ExprKind::Var {
        name: name.to_string(),
        indices,
        dim: None,
    })
}

pub fn upper(name: &str) -> Index {
    Index {
        name: name.to_string(),
        position: IndexPosition::Upper,
    }
}

pub fn lower(name: &str) -> Index {
    Index {
        name: name.to_string(),
        position: IndexPosition::Lower,
    }
}

/// Convert a float to a Rational expression. Handles integers and simple fractions.
/// Panics if the float cannot be represented as a rational with denominator ≤ 1000.
pub fn constant(n: f64) -> Expr {
    if n.fract() == 0.0 && n.abs() < (i64::MAX / 2) as f64 {
        return Expr::new(ExprKind::Rational(Rational::from_i64(n as i64)));
    }
    for d in 2..=1000i64 {
        let numerator = n * d as f64;
        let rounded = numerator.round();
        if (numerator - rounded).abs() < 1e-10 {
            return Expr::new(ExprKind::Rational(Rational::new(rounded as i64, d)));
        }
    }
    panic!("Cannot convert {} to Rational", n);
}

pub fn rational(n: i64, d: i64) -> Expr {
    Expr::new(ExprKind::Rational(Rational::new(n, d)))
}

pub fn frac_pi(n: i64, d: i64) -> Expr {
    Expr::new(ExprKind::FracPi(Rational::new(n, d)))
}

pub fn named(nc: NamedConst) -> Expr {
    Expr::new(ExprKind::Named(nc))
}

pub fn pi() -> Expr {
    Expr::new(ExprKind::FracPi(Rational::ONE))
}

pub fn e_const() -> Expr {
    Expr::new(ExprKind::Named(NamedConst::E))
}

pub fn add(a: Expr, b: Expr) -> Expr {
    Expr::new(ExprKind::Add(Box::new(a), Box::new(b)))
}

pub fn sub(a: Expr, b: Expr) -> Expr {
    Expr::new(ExprKind::Add(Box::new(a), Box::new(neg(b))))
}

pub fn mul(a: Expr, b: Expr) -> Expr {
    Expr::new(ExprKind::Mul(Box::new(a), Box::new(b)))
}

pub fn div(a: Expr, b: Expr) -> Expr {
    Expr::new(ExprKind::Mul(Box::new(a), Box::new(inv(b))))
}

pub fn neg(a: Expr) -> Expr {
    Expr::new(ExprKind::Neg(Box::new(a)))
}

pub fn inv(a: Expr) -> Expr {
    Expr::new(ExprKind::Inv(Box::new(a)))
}

pub fn pow(a: Expr, b: Expr) -> Expr {
    Expr::new(ExprKind::Pow(Box::new(a), Box::new(b)))
}

pub fn sqrt(a: Expr) -> Expr {
    pow(a, Expr::new(ExprKind::Rational(Rational::new(1, 2))))
}

pub fn sin(a: Expr) -> Expr {
    Expr::new(ExprKind::Fn(FnKind::Sin, Box::new(a)))
}

pub fn cos(a: Expr) -> Expr {
    Expr::new(ExprKind::Fn(FnKind::Cos, Box::new(a)))
}

pub fn tan(a: Expr) -> Expr {
    Expr::new(ExprKind::Fn(FnKind::Tan, Box::new(a)))
}

pub fn asin(a: Expr) -> Expr {
    Expr::new(ExprKind::Fn(FnKind::Asin, Box::new(a)))
}

pub fn acos(a: Expr) -> Expr {
    Expr::new(ExprKind::Fn(FnKind::Acos, Box::new(a)))
}

pub fn atan(a: Expr) -> Expr {
    Expr::new(ExprKind::Fn(FnKind::Atan, Box::new(a)))
}

pub fn sign(a: Expr) -> Expr {
    Expr::new(ExprKind::Fn(FnKind::Sign, Box::new(a)))
}

pub fn sinh(a: Expr) -> Expr {
    Expr::new(ExprKind::Fn(FnKind::Sinh, Box::new(a)))
}

pub fn cosh(a: Expr) -> Expr {
    Expr::new(ExprKind::Fn(FnKind::Cosh, Box::new(a)))
}

pub fn tanh(a: Expr) -> Expr {
    Expr::new(ExprKind::Fn(FnKind::Tanh, Box::new(a)))
}

pub fn floor(a: Expr) -> Expr {
    Expr::new(ExprKind::Fn(FnKind::Floor, Box::new(a)))
}

pub fn ceil(a: Expr) -> Expr {
    Expr::new(ExprKind::Fn(FnKind::Ceil, Box::new(a)))
}

pub fn round(a: Expr) -> Expr {
    Expr::new(ExprKind::Fn(FnKind::Round, Box::new(a)))
}

pub fn min(a: Expr, b: Expr) -> Expr {
    Expr::new(ExprKind::FnN(FnKind::Min, vec![a, b]))
}

pub fn max(a: Expr, b: Expr) -> Expr {
    Expr::new(ExprKind::FnN(FnKind::Max, vec![a, b]))
}

pub fn clamp(x: Expr, lo: Expr, hi: Expr) -> Expr {
    Expr::new(ExprKind::FnN(FnKind::Clamp, vec![x, lo, hi]))
}

pub fn exp(a: Expr) -> Expr {
    Expr::new(ExprKind::Fn(FnKind::Exp, Box::new(a)))
}

pub fn ln(a: Expr) -> Expr {
    Expr::new(ExprKind::Fn(FnKind::Ln, Box::new(a)))
}

pub fn quantity(expr: Expr, unit: Unit) -> Expr {
    Expr::new(ExprKind::Quantity(Box::new(expr), unit))
}

/// Returns true if the expression has tensor indices.
pub fn has_indices(expr: &Expr) -> bool {
    matches!(&expr.kind, ExprKind::Var { indices, .. } if !indices.is_empty())
}

/// Count index contractions between two expressions using Einstein notation.
/// A contraction occurs when the same index name appears with opposite positions
/// (one upper, one lower) in the two expressions.
fn count_contractions(left: &Expr, right: &Expr) -> usize {
    if let (
        ExprKind::Var {
            indices: left_indices,
            ..
        },
        ExprKind::Var {
            indices: right_indices,
            ..
        },
    ) = (&left.kind, &right.kind)
    {
        let mut count = 0;
        for li in left_indices {
            for ri in right_indices {
                if li.name == ri.name && li.position != ri.position {
                    count += 1;
                }
            }
        }
        count
    } else {
        0
    }
}

/// Determines the type of tensor multiplication based on Einstein notation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MulKind {
    /// Scalar multiplication (at least one operand has no indices)
    Scalar,
    /// Outer/tensor product (both have indices, none contract)
    Outer,
    /// Single contraction (dot product)
    Single,
    /// Double contraction
    Double,
}

/// Classify the multiplication type based on Einstein notation.
pub fn classify_mul(left: &Expr, right: &Expr) -> MulKind {
    let contractions = count_contractions(left, right);
    match contractions {
        0 if has_indices(left) && has_indices(right) => MulKind::Outer,
        0 => MulKind::Scalar,
        1 => MulKind::Single,
        _ => MulKind::Double,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dim::{BaseDim, Dimension};
    use crate::unit::Unit;

    fn test_unit(sym: &str) -> Unit {
        Unit {
            dimension: Dimension::single(BaseDim::L, 1),
            scale: 1.0,
            display: sym.to_string(),
        }
    }

    // --- collect_units ---

    #[test]
    fn collect_units_quantity() {
        let e = quantity(constant(5.0), test_unit("m"));
        assert_eq!(e.collect_units(), vec!["m"]);
    }

    #[test]
    fn collect_units_nested_add() {
        let e = add(
            quantity(constant(1.0), test_unit("m")),
            quantity(constant(2.0), test_unit("m")),
        );
        assert_eq!(e.collect_units(), vec!["m"]);
    }

    #[test]
    fn collect_units_mul() {
        let u1 = Unit {
            dimension: Dimension::single(BaseDim::L, 1),
            scale: 1.0,
            display: "m".to_string(),
        };
        let u2 = Unit {
            dimension: Dimension::single(BaseDim::T, 1),
            scale: 1.0,
            display: "s".to_string(),
        };
        let e = mul(quantity(constant(1.0), u1), quantity(constant(2.0), u2));
        assert_eq!(e.collect_units(), vec!["m", "s"]);
    }

    #[test]
    fn collect_units_neg() {
        let e = neg(quantity(constant(1.0), test_unit("m")));
        assert_eq!(e.collect_units(), vec!["m"]);
    }

    #[test]
    fn collect_units_inv() {
        let e = inv(quantity(constant(1.0), test_unit("m")));
        assert_eq!(e.collect_units(), vec!["m"]);
    }

    #[test]
    fn collect_units_fn() {
        let e = sin(quantity(constant(1.0), test_unit("rad")));
        assert_eq!(e.collect_units(), vec!["rad"]);
    }

    #[test]
    fn collect_units_fnn() {
        let e = min(
            quantity(constant(1.0), test_unit("m")),
            quantity(constant(2.0), test_unit("m")),
        );
        assert_eq!(e.collect_units(), vec!["m"]);
    }

    #[test]
    fn collect_units_no_units() {
        let e = add(scalar("x"), constant(1.0));
        assert!(e.collect_units().is_empty());
    }

    // --- first_unit ---

    #[test]
    fn first_unit_quantity() {
        let u = test_unit("m");
        let e = quantity(constant(5.0), u.clone());
        assert_eq!(e.first_unit(), Some(&u));
    }

    #[test]
    fn first_unit_in_add() {
        let u = test_unit("m");
        let e = add(scalar("x"), quantity(constant(1.0), u.clone()));
        assert_eq!(e.first_unit(), Some(&u));
    }

    #[test]
    fn first_unit_in_neg() {
        let u = test_unit("m");
        let e = neg(quantity(constant(1.0), u.clone()));
        assert_eq!(e.first_unit(), Some(&u));
    }

    #[test]
    fn first_unit_in_fnn() {
        let u = test_unit("m");
        let e = min(quantity(constant(1.0), u.clone()), constant(2.0));
        assert_eq!(e.first_unit(), Some(&u));
    }

    #[test]
    fn first_unit_none() {
        assert!(scalar("x").first_unit().is_none());
        assert!(constant(1.0).first_unit().is_none());
        assert!(pi().first_unit().is_none());
        assert!(e_const().first_unit().is_none());
    }

    // --- complexity ---

    #[test]
    fn complexity_atoms() {
        assert_eq!(scalar("x").complexity(), 1);
        assert_eq!(constant(1.0).complexity(), 1);
        assert_eq!(pi().complexity(), 1);
        assert_eq!(e_const().complexity(), 1);
    }

    #[test]
    fn complexity_quantity() {
        let e = quantity(constant(5.0), test_unit("m"));
        assert_eq!(e.complexity(), 2); // 1 for Quantity + 1 for inner
    }

    #[test]
    fn complexity_compound() {
        let e = add(scalar("x"), mul(scalar("y"), constant(2.0)));
        assert_eq!(e.complexity(), 5);
    }

    #[test]
    fn complexity_fnn() {
        let e = clamp(scalar("x"), constant(0.0), constant(1.0));
        assert_eq!(e.complexity(), 4);
    }

    // --- PartialEq ---

    #[test]
    fn eq_quantity() {
        let u = test_unit("m");
        let a = quantity(constant(5.0), u.clone());
        let b = quantity(constant(5.0), u);
        assert_eq!(a, b);
    }

    #[test]
    fn neq_different_types() {
        assert_ne!(scalar("x"), constant(1.0));
        assert_ne!(pi(), e_const());
    }

    #[test]
    fn strip_types_removes_inline_dims_and_ty() {
        let dim = Dimension::single(BaseDim::L, 1);
        let expr = Expr::spanned_typed(
            ExprKind::Var {
                name: "x".to_string(),
                indices: vec![],
                dim: Some(dim.clone()),
            },
            Span::new(2, 3),
            Ty::Concrete(dim),
        );

        let stripped = expr.strip_types();
        assert_eq!(stripped.span, Span::new(2, 3));
        assert_eq!(stripped.ty, Ty::Unresolved);
        match stripped.kind {
            ExprKind::Var { dim, .. } => assert_eq!(dim, None),
            other => panic!("expected stripped var, got {:?}", other),
        }
    }

    #[test]
    fn strip_types_unwraps_quantities() {
        let unit = test_unit("m");
        let expr = Expr::spanned_typed(
            ExprKind::Quantity(Box::new(constant(5.0)), unit.clone()),
            Span::new(0, 5),
            Ty::Concrete(unit.dimension.clone()),
        );

        let stripped = expr.strip_types();
        assert_eq!(stripped.span, Span::new(0, 5));
        assert_eq!(stripped.ty, Ty::Unresolved);
        assert_eq!(stripped, constant(5.0));
    }

    // --- NamedConst ---

    #[test]
    fn named_const_from_value_e() {
        assert_eq!(
            NamedConst::from_value(std::f64::consts::E),
            Some(NamedConst::E)
        );
    }

    #[test]
    fn named_const_from_value_sqrt2() {
        assert_eq!(
            NamedConst::from_value(std::f64::consts::SQRT_2),
            Some(NamedConst::Sqrt2)
        );
    }

    #[test]
    fn named_const_from_value_none() {
        assert_eq!(NamedConst::from_value(42.0), None);
    }

    // --- classify_mul ---

    #[test]
    fn classify_mul_scalar() {
        assert_eq!(classify_mul(&scalar("a"), &scalar("b")), MulKind::Scalar);
    }

    #[test]
    fn classify_mul_outer() {
        let a = tensor("A", vec![upper("i")]);
        let b = tensor("B", vec![upper("j")]);
        assert_eq!(classify_mul(&a, &b), MulKind::Outer);
    }

    #[test]
    fn classify_mul_single() {
        let a = tensor("A", vec![upper("i")]);
        let b = tensor("B", vec![lower("i")]);
        assert_eq!(classify_mul(&a, &b), MulKind::Single);
    }

    #[test]
    fn classify_mul_double() {
        let a = tensor("A", vec![upper("i"), upper("j")]);
        let b = tensor("B", vec![lower("i"), lower("j")]);
        assert_eq!(classify_mul(&a, &b), MulKind::Double);
    }
}
