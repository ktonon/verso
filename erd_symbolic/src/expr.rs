pub enum Expr {
    // Atoms
    Const(f64),
    Var(String),

    // Arithmetic
    Add(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Neg(Box<Expr>),
    Inv(Box<Expr>), // multiplicative inverse (1/x)
    Pow(Box<Expr>, Box<Expr>),

    // Functions
    Fn(FnKind, Box<Expr>),
}

pub enum FnKind {
    Sin,
    Cos,
    Exp,
    Ln,
    // extend as needed
}

impl std::fmt::Display for Expr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Expr::Const(n) => write!(f, "{}", n),
            Expr::Var(name) => write!(f, "{}", name),
            Expr::Add(a, b) => write!(f, "({} + {})", a, b),
            Expr::Mul(a, b) => write!(f, "({} * {})", a, b),
            Expr::Neg(a) => write!(f, "(-{})", a),
            Expr::Inv(a) => write!(f, "(1/{})", a),
            Expr::Pow(base, exp) => write!(f, "({})^({})", base, exp),
            Expr::Fn(kind, arg) => write!(f, "{}({})", kind, arg),
        }
    }
}

impl std::fmt::Display for FnKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FnKind::Sin => write!(f, "sin"),
            FnKind::Cos => write!(f, "cos"),
            FnKind::Exp => write!(f, "exp"),
            FnKind::Ln => write!(f, "ln"),
        }
    }
}

pub fn var(name: &str) -> Expr {
    Expr::Var(name.to_string())
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
    pow(a, inv(constant(2.0)))
}

pub fn sin(a: Expr) -> Expr {
    Expr::Fn(FnKind::Sin, Box::new(a))
}

// etc.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_const() {
        let e = constant(3.0);
        assert_eq!(format!("{}", e), "3");
    }

    #[test]
    fn display_var() {
        let e = var("x");
        assert_eq!(format!("{}", e), "x");
    }

    #[test]
    fn display_add() {
        let e = add(var("x"), constant(2.0));
        assert_eq!(format!("{}", e), "(x + 2)");
    }

    #[test]
    fn display_sub() {
        let e = sub(var("x"), constant(2.0));
        assert_eq!(format!("{}", e), "(x + (-2))");
    }

    #[test]
    fn display_mul() {
        let e = mul(var("x"), var("y"));
        assert_eq!(format!("{}", e), "(x * y)");
    }

    #[test]
    fn display_div() {
        let e = div(var("x"), var("y"));
        assert_eq!(format!("{}", e), "(x * (1/y))");
    }

    #[test]
    fn display_neg() {
        let e = neg(var("x"));
        assert_eq!(format!("{}", e), "(-x)");
    }

    #[test]
    fn display_inv() {
        let e = inv(var("x"));
        assert_eq!(format!("{}", e), "(1/x)");
    }

    #[test]
    fn display_pow() {
        let e = pow(var("x"), constant(2.0));
        assert_eq!(format!("{}", e), "(x)^(2)");
    }

    #[test]
    fn display_sqrt() {
        let e = sqrt(var("x"));
        assert_eq!(format!("{}", e), "(x)^((1/2))");
    }

    #[test]
    fn display_fn_sin() {
        let e = sin(var("x"));
        assert_eq!(format!("{}", e), "sin(x)");
    }

    #[test]
    fn display_nested() {
        // x^2 + 2x + 1
        let e = add(
            add(pow(var("x"), constant(2.0)), mul(constant(2.0), var("x"))),
            constant(1.0),
        );
        assert_eq!(format!("{}", e), "(((x)^(2) + (2 * x)) + 1)");
    }
}
