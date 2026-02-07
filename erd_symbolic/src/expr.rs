/// Least significant digit of a non-negative integer, enabling parity matching in rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LeastSigDigit {
    Zero,
    One,
    Two,
    Three,
    Four,
    Five,
    Six,
    Seven,
    Eight,
    Nine,
}

impl LeastSigDigit {
    pub fn value(self) -> i64 {
        match self {
            LeastSigDigit::Zero => 0,
            LeastSigDigit::One => 1,
            LeastSigDigit::Two => 2,
            LeastSigDigit::Three => 3,
            LeastSigDigit::Four => 4,
            LeastSigDigit::Five => 5,
            LeastSigDigit::Six => 6,
            LeastSigDigit::Seven => 7,
            LeastSigDigit::Eight => 8,
            LeastSigDigit::Nine => 9,
        }
    }

    pub fn from_i64(n: i64) -> Self {
        match (n.unsigned_abs()) % 10 {
            0 => LeastSigDigit::Zero,
            1 => LeastSigDigit::One,
            2 => LeastSigDigit::Two,
            3 => LeastSigDigit::Three,
            4 => LeastSigDigit::Four,
            5 => LeastSigDigit::Five,
            6 => LeastSigDigit::Six,
            7 => LeastSigDigit::Seven,
            8 => LeastSigDigit::Eight,
            9 => LeastSigDigit::Nine,
            _ => unreachable!(),
        }
    }

    pub fn is_even(self) -> bool {
        matches!(
            self,
            LeastSigDigit::Zero
                | LeastSigDigit::Two
                | LeastSigDigit::Four
                | LeastSigDigit::Six
                | LeastSigDigit::Eight
        )
    }

    pub fn is_odd(self) -> bool {
        !self.is_even()
    }
}

use crate::rational::Rational;

#[derive(Debug, Clone)]
pub enum Expr {
    // Atoms
    Const(f64),
    Integer(i64, LeastSigDigit), // value = hi * 10 + digit.value(), non-negative
    Rational(Rational),          // Exact rational number
    Named(NamedConst),           // Named mathematical constant (e, √2, etc.)
    FracPi(Rational),            // Rational multiple of π (value = r * π)
    Var { name: String, indices: Vec<Index> },

    // Arithmetic
    Add(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Neg(Box<Expr>),
    Inv(Box<Expr>), // multiplicative inverse (1/x)
    Pow(Box<Expr>, Box<Expr>),

    // Functions
    Fn(FnKind, Box<Expr>),
    FnN(FnKind, Vec<Expr>),
}

impl PartialEq for Expr {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Expr::Const(a), Expr::Const(b)) => a == b,
            (Expr::Integer(hi_a, lo_a), Expr::Integer(hi_b, lo_b)) => hi_a == hi_b && lo_a == lo_b,
            (Expr::Named(a), Expr::Named(b)) => a == b,
            (
                Expr::Var {
                    name: n1,
                    indices: i1,
                },
                Expr::Var {
                    name: n2,
                    indices: i2,
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
            // Cross-type: Const <-> Integer by value
            (Expr::Const(f), Expr::Integer(hi, lo)) | (Expr::Integer(hi, lo), Expr::Const(f)) => {
                let int_val = hi * 10 + lo.value();
                (*f - int_val as f64).abs() < f64::EPSILON
            }
            // Cross-type: Const <-> Rational by value
            (Expr::Const(f), Expr::Rational(r)) | (Expr::Rational(r), Expr::Const(f)) => {
                (*f - r.value()).abs() < 1e-12
            }
            // Cross-type: Integer <-> Rational by value
            (Expr::Integer(hi, lo), Expr::Rational(r))
            | (Expr::Rational(r), Expr::Integer(hi, lo)) => {
                r.is_integer() && r.num() == hi * 10 + lo.value()
            }
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Index {
    pub name: String,
    pub position: IndexPosition,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IndexPosition {
    Upper, // contravariant
    Lower, // covariant
}

#[derive(Debug, Clone, PartialEq)]
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
    // Pi-related
    Pi,       // π
    FracPi2,  // π/2
    FracPi3,  // π/3
    FracPi4,  // π/4
    FracPi6,  // π/6
    Frac2Pi3, // 2π/3
    Frac3Pi4, // 3π/4
    Frac5Pi4,  // 5π/4
    Frac5Pi6,  // 5π/6
    Frac7Pi6,  // 7π/6
    Frac3Pi2,  // 3π/2
    TwoPi,     // 2π
    // Other constants
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
            NamedConst::Pi => PI,
            NamedConst::FracPi2 => FRAC_PI_2,
            NamedConst::FracPi3 => FRAC_PI_3,
            NamedConst::FracPi4 => FRAC_PI_4,
            NamedConst::FracPi6 => FRAC_PI_6,
            NamedConst::Frac2Pi3 => 2.0 * FRAC_PI_3,
            NamedConst::Frac3Pi4 => 3.0 * FRAC_PI_4,
            NamedConst::Frac5Pi4 => 5.0 * FRAC_PI_4,
            NamedConst::Frac5Pi6 => 5.0 * FRAC_PI_6,
            NamedConst::Frac7Pi6 => 7.0 * FRAC_PI_6,
            NamedConst::Frac3Pi2 => 3.0 * FRAC_PI_2,
            NamedConst::TwoPi => TAU,
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
            (3.0 * FRAC_PI_2, NamedConst::Frac3Pi2),
            (7.0 * FRAC_PI_6, NamedConst::Frac7Pi6),
            (5.0 * FRAC_PI_6, NamedConst::Frac5Pi6),
            (5.0 * FRAC_PI_4, NamedConst::Frac5Pi4),
            (3.0 * FRAC_PI_4, NamedConst::Frac3Pi4),
            (2.0 * FRAC_PI_3, NamedConst::Frac2Pi3),
            (PI, NamedConst::Pi),
            (FRAC_PI_2, NamedConst::FracPi2),
            (FRAC_PI_3, NamedConst::FracPi3),
            (FRAC_PI_4, NamedConst::FracPi4),
            (FRAC_PI_6, NamedConst::FracPi6),
            (TAU, NamedConst::TwoPi),
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
            Expr::Const(_)
            | Expr::Integer(_, _)
            | Expr::Rational(_)
            | Expr::Named(_)
            | Expr::FracPi(_)
            | Expr::Var { .. } => 1,
            Expr::Neg(a) | Expr::Inv(a) | Expr::Fn(_, a) => 1 + a.complexity(),
            Expr::FnN(_, args) => 1 + args.iter().map(|a| a.complexity()).sum::<usize>(),
            Expr::Add(a, b) | Expr::Mul(a, b) | Expr::Pow(a, b) => {
                1 + a.complexity() + b.complexity()
            }
        }
    }

    pub fn precedence(&self) -> u8 {
        match self {
            Expr::Const(_)
            | Expr::Integer(_, _)
            | Expr::Rational(_)
            | Expr::Named(_)
            | Expr::FracPi(_)
            | Expr::Var { .. }
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
    }
}

pub fn tensor(name: &str, indices: Vec<Index>) -> Expr {
    Expr::Var {
        name: name.to_string(),
        indices,
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

pub fn constant(n: f64) -> Expr {
    Expr::Const(n)
}

/// Construct an Integer expression from a non-negative i64.
/// For negative values, use `neg(integer(n.abs()))`.
pub fn integer(n: i64) -> Expr {
    assert!(
        n >= 0,
        "Integer must be non-negative; use neg(integer(..)) for negatives"
    );
    Expr::Integer(n / 10, LeastSigDigit::from_i64(n))
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
    Expr::Named(NamedConst::Pi)
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
    pow(a, constant(0.5))
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
