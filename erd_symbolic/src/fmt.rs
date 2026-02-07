use crate::expr::{classify_mul, Expr, FnKind, Index, IndexPosition, MulKind, NamedConst};
use crate::rational::Rational;
use std::fmt::Display;

/// Format a rational multiple of π for display.
fn fmt_frac_pi(r: &Rational) -> String {
    let n = r.num();
    let d = r.den();
    match (n, d) {
        (0, _) => "0".to_string(),
        (1, 1) => "π".to_string(),
        (-1, 1) => "-π".to_string(),
        (_, 1) => format!("{}π", n),
        (1, _) => format!("π/{}", d),
        (-1, _) => format!("-π/{}", d),
        _ => format!("{}π/{}", n, d),
    }
}

// ANSI color codes for terminal output
mod color {
    pub const RESET: &str = "\x1b[0m";
    pub const DIM: &str = "\x1b[2m"; // Gray for operators
    pub const CYAN: &str = "\x1b[36m"; // Constants
    pub const MAGENTA: &str = "\x1b[35m"; // Named constants (π, e, √2)
    pub const YELLOW: &str = "\x1b[33m"; // 1st order tensors
    pub const BOLD_YELLOW: &str = "\x1b[1;33m"; // 2nd order tensors (bold yellow)
    pub const BLUE: &str = "\x1b[34m"; // Functions
}

/// Wrapper for colored display of expressions
pub struct Colored<'a>(pub &'a Expr);

impl<'a> Display for Colored<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", fmt_colored(self.0))
    }
}

/// Format an expression with ANSI colors
pub fn fmt_colored(expr: &Expr) -> String {
    match expr {
        Expr::Const(n) => {
            let s = if n.fract() == 0.0 {
                format!("{}", *n as i64)
            } else {
                format!("{}", n)
            };
            format!("{}{}{}", color::CYAN, s, color::RESET)
        }
        Expr::Rational(r) => {
            format!("{}{}{}", color::CYAN, r, color::RESET)
        }
        Expr::Named(nc) => {
            format!("{}{}{}", color::MAGENTA, nc, color::RESET)
        }
        Expr::FracPi(r) => {
            let s = fmt_frac_pi(r);
            format!("{}{}{}", color::MAGENTA, s, color::RESET)
        }
        Expr::Var { name, indices } => {
            let order = indices.len();
            let (start_color, end_color) = match order {
                0 => (color::YELLOW, color::RESET),      // Scalar: default
                1 => (color::YELLOW, color::RESET),      // 1st order: yellow
                _ => (color::BOLD_YELLOW, color::RESET), // 2nd+: bold yellow
            };

            if indices.is_empty() {
                format!("{}{}{}", start_color, name, end_color)
            } else {
                let upper: Vec<_> = indices
                    .iter()
                    .filter(|i| i.position == IndexPosition::Upper)
                    .map(|i| format!("{}", i))
                    .collect();
                let lower: Vec<_> = indices
                    .iter()
                    .filter(|i| i.position == IndexPosition::Lower)
                    .map(|i| format!("{}", i))
                    .collect();

                // Color the tensor name, dim the indices
                let mut result = format!("{}{}{}", start_color, name, color::RESET);
                if !lower.is_empty() {
                    result.push_str(&format!(
                        "{}_({}){}",
                        color::DIM,
                        lower.join(","),
                        color::RESET
                    ));
                }
                if !upper.is_empty() {
                    result.push_str(&format!(
                        "{}^({}){}",
                        color::DIM,
                        upper.join(","),
                        color::RESET
                    ));
                }
                result
            }
        }
        Expr::Add(a, b) => {
            let op = format!("{} + {}", color::DIM, color::RESET);
            match b.as_ref() {
                Expr::Neg(inner) => {
                    let op = format!("{} - {}", color::DIM, color::RESET);
                    format!("{}{}{}", fmt_colored(a), op, fmt_colored(inner))
                }
                Expr::Const(n) if *n < 0.0 => {
                    let op = format!("{} - {}", color::DIM, color::RESET);
                    format!(
                        "{}{}{}{}{}",
                        fmt_colored(a),
                        op,
                        color::CYAN,
                        fmt_const(-*n),
                        color::RESET
                    )
                }
                _ => {
                    format!("{}{}{}", fmt_colored(a), op, fmt_colored(b))
                }
            }
        }
        Expr::Mul(a, b) => {
            if let Some((base, arg)) = match_log_base(a, b) {
                return format!(
                    "{}log{}_{}{}{}({}){}",
                    color::BLUE,
                    color::RESET,
                    color::DIM,
                    fmt_log_base_colored(base),
                    color::BLUE,
                    fmt_colored(arg),
                    color::RESET
                );
            }

            match b.as_ref() {
                Expr::Inv(inner) => {
                    let op = format!("{}/{}", color::DIM, color::RESET);
                    format!(
                        "{}{}{}",
                        maybe_paren_colored(a, expr),
                        op,
                        maybe_paren_colored(inner, expr)
                    )
                }
                _ => {
                    let mul_kind = classify_mul(a, b);
                    // Coefficient notation: 2x instead of 2 * x
                    if mul_kind == MulKind::Scalar {
                        if let Expr::Const(n) = a.as_ref() {
                            let coeff = if n.fract() == 0.0 {
                                format!("{}", *n as i64)
                            } else {
                                format!("{}", n)
                            };
                            return format!(
                                "{}{}{}{}",
                                color::CYAN,
                                coeff,
                                color::RESET,
                                maybe_paren_colored(b, expr)
                            );
                        }
                        if let Expr::Rational(r) = a.as_ref() {
                            return format!(
                                "{}{}{}{}",
                                color::CYAN,
                                r,
                                color::RESET,
                                maybe_paren_colored(b, expr)
                            );
                        }
                    }
                    let op_char = match mul_kind {
                        MulKind::Scalar => "⋅",
                        MulKind::Outer => "⊗",
                        MulKind::Single => "⋅",
                        MulKind::Double => ":",
                    };
                    let op = format!("{}{}{}", color::DIM, op_char, color::RESET);
                    format!(
                        "{}{}{}",
                        maybe_paren_colored(a, expr),
                        op,
                        maybe_paren_colored(b, expr)
                    )
                }
            }
        }
        Expr::Neg(a) => {
            format!(
                "{}-{}{}",
                color::DIM,
                color::RESET,
                maybe_paren_colored(a, expr)
            )
        }
        Expr::Inv(a) => {
            format!(
                "{}1/{}{}",
                color::DIM,
                color::RESET,
                maybe_paren_colored(a, expr)
            )
        }
        Expr::Pow(base, exp) => {
            if is_sqrt_exp(exp) {
                return format!("{}sqrt({}){}", color::BLUE, fmt_colored(base), color::RESET);
            }
            let op = format!("{}**{}", color::DIM, color::RESET);
            format!(
                "{}{}{}",
                maybe_paren_colored(base, expr),
                op,
                maybe_paren_colored(exp, expr)
            )
        }
        Expr::Fn(kind, arg) => {
            format!(
                "{}{}{}({})",
                color::BLUE,
                kind,
                color::RESET,
                fmt_colored(arg)
            )
        }
        Expr::FnN(kind, args) => {
            let rendered: Vec<String> = args.iter().map(fmt_colored).collect();
            format!(
                "{}{}{}({})",
                color::BLUE,
                kind,
                color::RESET,
                rendered.join(", ")
            )
        }
    }
}

fn maybe_paren_colored(child: &Expr, parent: &Expr) -> String {
    if child.precedence() < parent.precedence() {
        format!(
            "{}({}{}){}",
            color::DIM,
            color::RESET,
            fmt_colored(child),
            color::DIM
        )
    } else {
        fmt_colored(child)
    }
}

fn fmt_log_base_colored(base: &Expr) -> String {
    match base {
        Expr::Const(_) | Expr::Rational(_) | Expr::Var { .. } => {
            fmt_colored(base)
        }
        _ => format!(
            "{}({}{}){}",
            color::DIM,
            color::RESET,
            fmt_colored(base),
            color::DIM
        ),
    }
}

impl Display for NamedConst {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NamedConst::E => write!(f, "e"),
            NamedConst::Sqrt2 => write!(f, "√2"),
            NamedConst::Sqrt3 => write!(f, "√3"),
            NamedConst::Frac1Sqrt2 => write!(f, "√2/2"),
            NamedConst::FracSqrt3By2 => write!(f, "√3/2"),
        }
    }
}

impl Display for Index {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl Display for FnKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FnKind::Sin => write!(f, "sin"),
            FnKind::Cos => write!(f, "cos"),
            FnKind::Tan => write!(f, "tan"),
            FnKind::Asin => write!(f, "asin"),
            FnKind::Acos => write!(f, "acos"),
            FnKind::Atan => write!(f, "atan"),
            FnKind::Sign => write!(f, "sign"),
            FnKind::Sinh => write!(f, "sinh"),
            FnKind::Cosh => write!(f, "cosh"),
            FnKind::Tanh => write!(f, "tanh"),
            FnKind::Floor => write!(f, "floor"),
            FnKind::Ceil => write!(f, "ceil"),
            FnKind::Round => write!(f, "round"),
            FnKind::Min => write!(f, "min"),
            FnKind::Max => write!(f, "max"),
            FnKind::Clamp => write!(f, "clamp"),
            FnKind::Exp => write!(f, "exp"),
            FnKind::Ln => write!(f, "ln"),
        }
    }
}

impl std::fmt::Display for Expr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Expr::Const(n) => {
                if n.fract() == 0.0 {
                    write!(f, "{}", *n as i64)
                } else {
                    write!(f, "{}", n)
                }
            }
            Expr::Rational(r) => write!(f, "{}", r),
            Expr::Named(nc) => write!(f, "{}", nc),
            Expr::FracPi(r) => write!(f, "{}", fmt_frac_pi(r)),
            Expr::Var { name, indices } => {
                if indices.is_empty() {
                    write!(f, "{}", name)
                } else {
                    let upper: Vec<_> = indices
                        .iter()
                        .filter(|i| i.position == IndexPosition::Upper)
                        .map(|i| format!("{}", i))
                        .collect();
                    let lower: Vec<_> = indices
                        .iter()
                        .filter(|i| i.position == IndexPosition::Lower)
                        .map(|i| format!("{}", i))
                        .collect();

                    let mut result = name.clone();
                    if !lower.is_empty() {
                        result.push_str(&format!("_({})", lower.join(",")));
                    }
                    if !upper.is_empty() {
                        result.push_str(&format!("^({})", upper.join(",")));
                    }
                    write!(f, "{}", result)
                }
            }
            Expr::Add(a, b) => match b.as_ref() {
                Expr::Neg(inner) => {
                    write!(f, "{} - {}", a, inner)
                }
                Expr::Const(n) if *n < 0.0 => {
                    write!(f, "{} - {}", a, fmt_const(-*n))
                }
                _ => {
                    write!(f, "{} + {}", a, b)
                }
            },
            Expr::Mul(a, b) => {
                if let Some((base, arg)) = match_log_base(a, b) {
                    return write!(f, "log_{}({})", fmt_log_base(base), arg);
                }

                match b.as_ref() {
                    Expr::Inv(inner) => {
                        write!(f, "{}/{}", maybe_paren(a, self), maybe_paren(inner, self))
                    }
                    _ => {
                        let mul_kind = classify_mul(a, b);
                        // Coefficient notation: 2x instead of 2 * x
                        if mul_kind == MulKind::Scalar {
                            if let Expr::Const(n) = a.as_ref() {
                                return write!(f, "{}{}", n, maybe_paren(b, self));
                            }
                            if let Expr::Rational(r) = a.as_ref() {
                                return write!(f, "{}{}", r, maybe_paren(b, self));
                            }
                        }
                        // Choose operator based on multiplication kind (Einstein notation)
                        // No spaces around operators for tighter visual binding
                        let op = match mul_kind {
                            MulKind::Scalar => "⋅", // scalar multiplication
                            MulKind::Outer => "⊗",  // outer/tensor product
                            MulKind::Single => "⋅", // single contraction (dot product)
                            MulKind::Double => ":", // double contraction
                        };
                        write!(f, "{}{}{}", maybe_paren(a, self), op, maybe_paren(b, self))
                    }
                }
            }
            Expr::Neg(a) => write!(f, "-{}", maybe_paren(a, self)),
            Expr::Inv(a) => write!(f, "1/{}", maybe_paren(a, self)),
            Expr::Pow(base, exp) => {
                if is_sqrt_exp(exp) {
                    return write!(f, "sqrt({})", base);
                }
                write!(f, "{}**{}", maybe_paren(base, self), maybe_paren(exp, self))
            }
            Expr::Fn(kind, arg) => write!(f, "{}({})", kind, arg),
            Expr::FnN(kind, args) => {
                let rendered: Vec<String> = args.iter().map(|a| format!("{}", a)).collect();
                write!(f, "{}({})", kind, rendered.join(", "))
            }
        }
    }
}

fn maybe_paren(child: &Expr, parent: &Expr) -> String {
    if child.precedence() < parent.precedence() {
        format!("({})", child)
    } else {
        format!("{}", child)
    }
}

fn fmt_const(n: f64) -> String {
    if n.fract() == 0.0 {
        format!("{}", n as i64)
    } else {
        format!("{}", n)
    }
}

fn is_sqrt_exp(exp: &Expr) -> bool {
    match exp {
        Expr::Const(n) => *n == 0.5,
        Expr::Rational(_) | Expr::FracPi(_) => false,
        Expr::Inv(inner) => matches!(inner.as_ref(), Expr::Const(n) if *n == 2.0),
        _ => false,
    }
}

fn match_log_base<'a>(left: &'a Expr, right: &'a Expr) -> Option<(&'a Expr, &'a Expr)> {
    match (left, right) {
        (Expr::Fn(FnKind::Ln, arg), Expr::Inv(inner)) => match inner.as_ref() {
            Expr::Fn(FnKind::Ln, base) => Some((base.as_ref(), arg.as_ref())),
            _ => None,
        },
        (Expr::Inv(inner), Expr::Fn(FnKind::Ln, arg)) => match inner.as_ref() {
            Expr::Fn(FnKind::Ln, base) => Some((base.as_ref(), arg.as_ref())),
            _ => None,
        },
        _ => None,
    }
}

fn fmt_log_base(base: &Expr) -> String {
    match base {
        Expr::Const(_) | Expr::Rational(_) | Expr::Var { .. } => {
            format!("{}", base)
        }
        _ => format!("({})", base),
    }
}

#[cfg(test)]
mod tests {
    use crate::expr::*;

    #[test]
    fn display_const() {
        assert_eq!(format!("{}", constant(3.0)), "3");
        assert_eq!(format!("{}", constant(3.5)), "3.5");
        assert_eq!(format!("{}", constant(-3.5)), "-3.5");
    }

    #[test]
    fn display_scalar() {
        assert_eq!(format!("{}", scalar("x")), "x");
    }

    #[test]
    fn display_tensor() {
        assert_eq!(format!("{}", tensor("X", vec![lower("i")])), "X_(i)");
        assert_eq!(format!("{}", tensor("X", vec![upper("i")])), "X^(i)");
        assert_eq!(
            format!("{}", tensor("X", vec![lower("i"), lower("j"), upper("k")])),
            "X_(i,j)^(k)"
        );
    }

    #[test]
    fn display_add() {
        assert_eq!(format!("{}", add(scalar("x"), constant(2.0))), "x + 2");
        assert_eq!(format!("{}", add(scalar("x"), constant(-2.0))), "x - 2");
    }

    #[test]
    fn display_sub() {
        assert_eq!(format!("{}", sub(scalar("x"), constant(2.0))), "x - 2");
    }

    #[test]
    fn display_mul_scalar() {
        // Scalar multiplication (no indices, no contractions)
        assert_eq!(format!("{}", mul(scalar("x"), scalar("y"))), "x⋅y");
        // Coefficient notation preserved
        assert_eq!(format!("{}", mul(constant(2.0), scalar("y"))), "2y");
    }

    #[test]
    fn display_mul_single_contraction() {
        // A^i B_i contracts on i → single contraction → dot
        let e = mul(tensor("A", vec![upper("i")]), tensor("B", vec![lower("i")]));
        assert_eq!(format!("{}", e), "A^(i)⋅B_(i)");
    }

    #[test]
    fn display_mul_double_contraction() {
        // A^ij B_ij contracts on both i and j → double contraction → colon
        let e = mul(
            tensor("A", vec![upper("i"), upper("j")]),
            tensor("B", vec![lower("i"), lower("j")]),
        );
        assert_eq!(format!("{}", e), "A^(i,j):B_(i,j)");
    }

    #[test]
    fn display_mul_outer_product() {
        // A^i B^j has no contractions (both upper) → outer/tensor product
        let e = mul(tensor("A", vec![upper("i")]), tensor("B", vec![upper("j")]));
        assert_eq!(format!("{}", e), "A^(i)⊗B^(j)");
    }

    #[test]
    fn display_div() {
        assert_eq!(format!("{}", div(scalar("x"), scalar("y"))), "x/y");
    }

    #[test]
    fn display_neg() {
        assert_eq!(
            format!("{}", neg(add(constant(2.5), scalar("x")))),
            "-(2.5 + x)"
        );
        assert_eq!(format!("{}", neg(scalar("x"))), "-x");
    }

    #[test]
    fn display_inv() {
        assert_eq!(format!("{}", inv(scalar("x"))), "1/x");
        assert_eq!(
            format!("{}", inv(add(constant(2.5), scalar("x")))),
            "1/(2.5 + x)"
        );
    }

    #[test]
    fn display_pow() {
        assert_eq!(format!("{}", pow(scalar("x"), constant(2.0))), "x**2");
        assert_eq!(
            format!("{}", pow(scalar("y"), add(constant(2.0), scalar("x")))),
            "y**(2 + x)"
        );
        assert_eq!(
            format!("{}", pow(add(constant(2.0), scalar("x")), scalar("y"))),
            "(2 + x)**y"
        );
    }

    #[test]
    fn display_sqrt() {
        let e = sqrt(scalar("x"));
        assert_eq!(format!("{}", e), "sqrt(x)");
    }

    #[test]
    fn display_fn_sin() {
        let e = sin(scalar("x"));
        assert_eq!(format!("{}", e), "sin(x)");
    }

    #[test]
    fn display_fn_trig_inv() {
        assert_eq!(format!("{}", tan(scalar("x"))), "tan(x)");
        assert_eq!(format!("{}", asin(scalar("x"))), "asin(x)");
        assert_eq!(format!("{}", acos(scalar("x"))), "acos(x)");
        assert_eq!(format!("{}", atan(scalar("x"))), "atan(x)");
    }

    #[test]
    fn display_fn_misc() {
        assert_eq!(format!("{}", sign(scalar("x"))), "sign(x)");
        assert_eq!(format!("{}", sinh(scalar("x"))), "sinh(x)");
        assert_eq!(format!("{}", cosh(scalar("x"))), "cosh(x)");
        assert_eq!(format!("{}", tanh(scalar("x"))), "tanh(x)");
        assert_eq!(format!("{}", floor(scalar("x"))), "floor(x)");
        assert_eq!(format!("{}", ceil(scalar("x"))), "ceil(x)");
        assert_eq!(format!("{}", round(scalar("x"))), "round(x)");
    }

    #[test]
    fn display_fn_multi_arg() {
        assert_eq!(format!("{}", min(scalar("a"), scalar("b"))), "min(a, b)");
        assert_eq!(format!("{}", max(scalar("a"), scalar("b"))), "max(a, b)");
        assert_eq!(
            format!("{}", clamp(scalar("x"), constant(0.0), constant(1.0))),
            "clamp(x, 0, 1)"
        );
    }

    #[test]
    fn display_log_base() {
        let e = mul(ln(scalar("x")), inv(ln(constant(10.0))));
        assert_eq!(format!("{}", e), "log_10(x)");
    }

    #[test]
    fn display_nested() {
        // x^2 + 2x + 1
        let e = add(
            add(
                pow(scalar("x"), constant(2.0)),
                mul(constant(2.0), scalar("x")),
            ),
            constant(1.0),
        );
        assert_eq!(format!("{}", e), "x**2 + 2x + 1");
    }
}
