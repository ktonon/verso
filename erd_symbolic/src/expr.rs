use crate::dim::Dimension;
use crate::rational::Rational;
use crate::unit::Unit;

#[derive(Debug, Clone)]
pub enum Expr {
    // Atoms
    Rational(Rational), // Exact rational number
    FracPi(Rational),   // Rational multiple of π (value = r * π)
    Named(NamedConst),  // Named mathematical constant (e, √2, etc.)
    Var { name: String, indices: Vec<Index>, dim: Option<Dimension> },

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

impl PartialEq for Expr {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Expr::Named(a), Expr::Named(b)) => a == b,
            (
                Expr::Var {
                    name: n1,
                    indices: i1,
                    ..
                },
                Expr::Var {
                    name: n2,
                    indices: i2,
                    ..
                },
            ) => n1 == n2 && i1 == i2,
            (Expr::Add(a1, b1), Expr::Add(a2, b2))
            | (Expr::Mul(a1, b1), Expr::Mul(a2, b2))
            | (Expr::Pow(a1, b1), Expr::Pow(a2, b2)) => a1 == a2 && b1 == b2,
            (Expr::Neg(a), Expr::Neg(b)) | (Expr::Inv(a), Expr::Inv(b)) => a == b,
            (Expr::Fn(k1, a), Expr::Fn(k2, b)) => k1 == k2 && a == b,
            (Expr::FnN(k1, a), Expr::FnN(k2, b)) => k1 == k2 && a == b,
            (Expr::Rational(a), Expr::Rational(b)) => a == b,
            (Expr::FracPi(a), Expr::FracPi(b)) => a == b,
            (Expr::Quantity(a1, u1), Expr::Quantity(a2, u2)) => a1 == a2 && u1 == u2,
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
    // extend as needed
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
        match self {
            Expr::Rational(_) | Expr::Named(_) | Expr::FracPi(_) | Expr::Var { .. } => 1,
            Expr::Quantity(inner, _) => 1 + inner.complexity(),
            Expr::Neg(a) | Expr::Inv(a) | Expr::Fn(_, a) => 1 + a.complexity(),
            Expr::FnN(_, args) => 1 + args.iter().map(|a| a.complexity()).sum::<usize>(),
            Expr::Add(a, b) | Expr::Mul(a, b) | Expr::Pow(a, b) => {
                1 + a.complexity() + b.complexity()
            }
        }
    }

    pub fn precedence(&self) -> u8 {
        match self {
            Expr::Rational(_)
            | Expr::Named(_)
            | Expr::FracPi(_)
            | Expr::Var { .. }
            | Expr::Quantity(_, _)
            | Expr::Fn(_, _)
            | Expr::FnN(_, _) => 100,
            Expr::Pow(_, _) => 80,
            Expr::Neg(_) | Expr::Inv(_) => 70,
            Expr::Mul(_, _) => 60,
            Expr::Add(_, _) => 50,
        }
    }
}

pub fn scalar(name: &str) -> Expr {
    Expr::Var {
        name: name.to_string(),
        indices: vec![],
        dim: None,
    }
}

pub fn scalar_dim(name: &str, dim: Dimension) -> Expr {
    Expr::Var {
        name: name.to_string(),
        indices: vec![],
        dim: Some(dim),
    }
}

pub fn tensor(name: &str, indices: Vec<Index>) -> Expr {
    Expr::Var {
        name: name.to_string(),
        indices,
        dim: None,
    }
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
        return Expr::Rational(Rational::from_i64(n as i64));
    }
    for d in 2..=1000i64 {
        let numerator = n * d as f64;
        let rounded = numerator.round();
        if (numerator - rounded).abs() < 1e-10 {
            return Expr::Rational(Rational::new(rounded as i64, d));
        }
    }
    panic!("Cannot convert {} to Rational", n);
}

pub fn rational(n: i64, d: i64) -> Expr {
    Expr::Rational(Rational::new(n, d))
}

pub fn frac_pi(n: i64, d: i64) -> Expr {
    Expr::FracPi(Rational::new(n, d))
}

pub fn named(nc: NamedConst) -> Expr {
    Expr::Named(nc)
}

pub fn pi() -> Expr {
    Expr::FracPi(Rational::ONE)
}

pub fn e_const() -> Expr {
    Expr::Named(NamedConst::E)
}

pub fn add(a: Expr, b: Expr) -> Expr {
    Expr::Add(Box::new(a), Box::new(b))
}

pub fn sub(a: Expr, b: Expr) -> Expr {
    Expr::Add(Box::new(a), Box::new(neg(b)))
}

pub fn mul(a: Expr, b: Expr) -> Expr {
    Expr::Mul(Box::new(a), Box::new(b))
}

pub fn div(a: Expr, b: Expr) -> Expr {
    Expr::Mul(Box::new(a), Box::new(inv(b)))
}

pub fn neg(a: Expr) -> Expr {
    Expr::Neg(Box::new(a))
}

pub fn inv(a: Expr) -> Expr {
    Expr::Inv(Box::new(a))
}

pub fn pow(a: Expr, b: Expr) -> Expr {
    Expr::Pow(Box::new(a), Box::new(b))
}

pub fn sqrt(a: Expr) -> Expr {
    pow(a, Expr::Rational(Rational::new(1, 2)))
}

pub fn sin(a: Expr) -> Expr {
    Expr::Fn(FnKind::Sin, Box::new(a))
}

pub fn cos(a: Expr) -> Expr {
    Expr::Fn(FnKind::Cos, Box::new(a))
}

pub fn tan(a: Expr) -> Expr {
    Expr::Fn(FnKind::Tan, Box::new(a))
}

pub fn asin(a: Expr) -> Expr {
    Expr::Fn(FnKind::Asin, Box::new(a))
}

pub fn acos(a: Expr) -> Expr {
    Expr::Fn(FnKind::Acos, Box::new(a))
}

pub fn atan(a: Expr) -> Expr {
    Expr::Fn(FnKind::Atan, Box::new(a))
}

pub fn sign(a: Expr) -> Expr {
    Expr::Fn(FnKind::Sign, Box::new(a))
}

pub fn sinh(a: Expr) -> Expr {
    Expr::Fn(FnKind::Sinh, Box::new(a))
}

pub fn cosh(a: Expr) -> Expr {
    Expr::Fn(FnKind::Cosh, Box::new(a))
}

pub fn tanh(a: Expr) -> Expr {
    Expr::Fn(FnKind::Tanh, Box::new(a))
}

pub fn floor(a: Expr) -> Expr {
    Expr::Fn(FnKind::Floor, Box::new(a))
}

pub fn ceil(a: Expr) -> Expr {
    Expr::Fn(FnKind::Ceil, Box::new(a))
}

pub fn round(a: Expr) -> Expr {
    Expr::Fn(FnKind::Round, Box::new(a))
}

pub fn min(a: Expr, b: Expr) -> Expr {
    Expr::FnN(FnKind::Min, vec![a, b])
}

pub fn max(a: Expr, b: Expr) -> Expr {
    Expr::FnN(FnKind::Max, vec![a, b])
}

pub fn clamp(x: Expr, lo: Expr, hi: Expr) -> Expr {
    Expr::FnN(FnKind::Clamp, vec![x, lo, hi])
}

pub fn exp(a: Expr) -> Expr {
    Expr::Fn(FnKind::Exp, Box::new(a))
}

pub fn ln(a: Expr) -> Expr {
    Expr::Fn(FnKind::Ln, Box::new(a))
}

pub fn quantity(expr: Expr, unit: Unit) -> Expr {
    Expr::Quantity(Box::new(expr), unit)
}

/// Returns true if the expression has tensor indices.
pub fn has_indices(expr: &Expr) -> bool {
    matches!(expr, Expr::Var { indices, .. } if !indices.is_empty())
}

/// Count index contractions between two expressions using Einstein notation.
/// A contraction occurs when the same index name appears with opposite positions
/// (one upper, one lower) in the two expressions.
fn count_contractions(left: &Expr, right: &Expr) -> usize {
    if let (
        Expr::Var {
            indices: left_indices,
            ..
        },
        Expr::Var {
            indices: right_indices,
            ..
        },
    ) = (left, right)
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
