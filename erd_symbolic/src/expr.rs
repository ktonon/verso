#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    // Atoms
    Const(f64),
    Var { name: String, indices: Vec<Index> },

    // Arithmetic
    Add(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Neg(Box<Expr>),
    Inv(Box<Expr>), // multiplicative inverse (1/x)
    Pow(Box<Expr>, Box<Expr>),

    // Functions
    Fn(FnKind, Box<Expr>),
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
    Exp,
    Ln,
    // extend as needed
}

impl Expr {
    pub fn complexity(&self) -> usize {
        match self {
            Expr::Const(_) | Expr::Var { .. } => 1,
            Expr::Neg(a) | Expr::Inv(a) | Expr::Fn(_, a) => 1 + a.complexity(),
            Expr::Add(a, b) | Expr::Mul(a, b) | Expr::Pow(a, b) => {
                1 + a.complexity() + b.complexity()
            }
        }
    }

    pub fn precedence(&self) -> u8 {
        match self {
            Expr::Const(_) | Expr::Var { .. } | Expr::Fn(_, _) => 100,
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
