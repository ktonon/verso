use crate::expr::{count_contractions, Expr, FnKind, Index, IndexPosition};
use std::fmt::Display;

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
                        result.push_str(&format!("_{}", lower.join("")));
                    }
                    if !upper.is_empty() {
                        result.push_str(&format!("^{}", upper.join("")));
                    }
                    write!(f, "{}", result)
                }
            }
            Expr::Add(a, b) => match b.as_ref() {
                Expr::Neg(inner) => {
                    write!(f, "{} - {}", a, inner)
                }
                _ => {
                    write!(f, "{} + {}", a, b)
                }
            },
            Expr::Mul(a, b) => match b.as_ref() {
                Expr::Inv(inner) => {
                    write!(f, "{} / {}", maybe_paren(a, self), maybe_paren(inner, self))
                }
                _ => {
                    let contractions = count_contractions(a, b);
                    // Coefficient notation: 2x instead of 2 * x
                    if let Expr::Const(n) = a.as_ref() {
                        if contractions == 0 {
                            return write!(f, "{}{}", n, maybe_paren(b, self));
                        }
                    }
                    // Choose operator based on contraction count (Einstein notation)
                    let op = match contractions {
                        0 => " * ", // scalar multiplication
                        1 => " ⋅ ", // single contraction (dot product)
                        _ => " : ", // double contraction
                    };
                    write!(f, "{}{}{}", maybe_paren(a, self), op, maybe_paren(b, self))
                }
            },
            Expr::Neg(a) => write!(f, "-{}", maybe_paren(a, self)),
            Expr::Inv(a) => write!(f, "1/{}", maybe_paren(a, self)),
            Expr::Pow(base, exp) => {
                write!(f, "{}**{}", maybe_paren(base, self), maybe_paren(exp, self))
            }
            Expr::Fn(kind, arg) => write!(f, "{}({})", kind, arg),
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
        assert_eq!(format!("{}", tensor("X", vec![lower("i")])), "X_i");
        assert_eq!(format!("{}", tensor("X", vec![upper("i")])), "X^i");
        assert_eq!(
            format!("{}", tensor("X", vec![lower("i"), lower("j"), upper("k")])),
            "X_ij^k"
        );
    }

    #[test]
    fn display_add() {
        assert_eq!(format!("{}", add(scalar("x"), constant(2.0))), "x + 2");
    }

    #[test]
    fn display_sub() {
        assert_eq!(format!("{}", sub(scalar("x"), constant(2.0))), "x - 2");
    }

    #[test]
    fn display_mul_scalar() {
        // Scalar multiplication (no indices, no contractions)
        assert_eq!(format!("{}", mul(scalar("x"), scalar("y"))), "x * y");
        // Coefficient notation preserved
        assert_eq!(format!("{}", mul(constant(2.0), scalar("y"))), "2y");
    }

    #[test]
    fn display_mul_single_contraction() {
        // A^i B_i contracts on i → single contraction → dot
        let e = mul(tensor("A", vec![upper("i")]), tensor("B", vec![lower("i")]));
        assert_eq!(format!("{}", e), "A^i ⋅ B_i");
    }

    #[test]
    fn display_mul_double_contraction() {
        // A^ij B_ij contracts on both i and j → double contraction → colon
        let e = mul(
            tensor("A", vec![upper("i"), upper("j")]),
            tensor("B", vec![lower("i"), lower("j")]),
        );
        assert_eq!(format!("{}", e), "A^ij : B_ij");
    }

    #[test]
    fn display_mul_no_contraction_tensors() {
        // A^i B^j has no contractions (both upper) → scalar multiplication
        let e = mul(tensor("A", vec![upper("i")]), tensor("B", vec![upper("j")]));
        assert_eq!(format!("{}", e), "A^i * B^j");
    }

    #[test]
    fn display_div() {
        assert_eq!(format!("{}", div(scalar("x"), scalar("y"))), "x / y");
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
        assert_eq!(format!("{}", e), "x**(1/2)");
    }

    #[test]
    fn display_fn_sin() {
        let e = sin(scalar("x"));
        assert_eq!(format!("{}", e), "sin(x)");
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
