use crate::expr::{classify_mul, Expr, FnKind, Index, IndexPosition, MulKind};

pub trait ToTex {
    fn to_tex(&self) -> String;
}

impl ToTex for Index {
    fn to_tex(&self) -> String {
        self.name.clone()
    }
}

impl ToTex for FnKind {
    fn to_tex(&self) -> String {
        match self {
            FnKind::Sin => "\\sin".to_string(),
            FnKind::Cos => "\\cos".to_string(),
            FnKind::Tan => "\\tan".to_string(),
            FnKind::Asin => "\\arcsin".to_string(),
            FnKind::Acos => "\\arccos".to_string(),
            FnKind::Atan => "\\arctan".to_string(),
            FnKind::Exp => "\\exp".to_string(),
            FnKind::Ln => "\\ln".to_string(),
        }
    }
}

impl ToTex for Expr {
    fn to_tex(&self) -> String {
        match self {
            Expr::Const(n) => {
                if n.fract() == 0.0 {
                    format!("{}", *n as i64)
                } else {
                    format!("{}", n)
                }
            }
            Expr::Var { name, indices } => {
                if indices.is_empty() {
                    name.clone()
                } else {
                    let upper: Vec<_> = indices
                        .iter()
                        .filter(|i| i.position == IndexPosition::Upper)
                        .map(|i| i.to_tex())
                        .collect();
                    let lower: Vec<_> = indices
                        .iter()
                        .filter(|i| i.position == IndexPosition::Lower)
                        .map(|i| i.to_tex())
                        .collect();

                    let mut result = name.clone();
                    if !upper.is_empty() {
                        result.push_str(&format!("^{{{}}}", upper.join("")));
                    }
                    if !lower.is_empty() {
                        result.push_str(&format!("_{{{}}}", lower.join("")));
                    }
                    result
                }
            }
            Expr::Add(a, b) => {
                // Detect subtraction: Add(a, Neg(b)) -> a - b
                match b.as_ref() {
                    Expr::Neg(inner) => {
                        format!("{} - {}", a.to_tex(), inner.to_tex())
                    }
                    _ => {
                        format!("{} + {}", a.to_tex(), b.to_tex())
                    }
                }
            }
            Expr::Mul(a, b) => {
                if let Some((base, arg)) = match_log_base(a, b) {
                    return format!("\\log_{{{}}}{{{}}}", base.to_tex(), arg.to_tex());
                }

                // Detect division: Mul(a, Inv(b)) -> \frac{a}{b}
                match b.as_ref() {
                    Expr::Inv(inner) => {
                        format!("\\frac{{{}}}{{{}}}", a.to_tex(), inner.to_tex())
                    }
                    _ => {
                        let a_tex = maybe_paren(a, self);
                        let b_tex = maybe_paren(b, self);
                        // Choose operator based on multiplication kind (Einstein notation)
                        let op = match classify_mul(a, b) {
                            MulKind::Scalar => " ",         // scalar multiplication (juxtaposition)
                            MulKind::Outer => " \\otimes ", // outer/tensor product
                            MulKind::Single => " \\cdot ",  // single contraction (dot product)
                            MulKind::Double => " : ",       // double contraction
                        };
                        format!("{}{}{}", a_tex, op, b_tex)
                    }
                }
            }
            Expr::Neg(a) => {
                format!("-{}", maybe_paren(a, self))
            }
            Expr::Inv(a) => {
                format!("\\frac{{1}}{{{}}}", a.to_tex())
            }
            Expr::Pow(base, exp) => {
                if is_sqrt_exp(exp) {
                    return format!("\\sqrt{{{}}}", base.to_tex());
                }
                format!("{}^{{{}}}", maybe_paren(base, self), exp.to_tex())
            }
            Expr::Fn(kind, arg) => {
                format!("{}{{{}}}", kind.to_tex(), arg.to_tex())
            }
        }
    }
}

fn maybe_paren(child: &Expr, parent: &Expr) -> String {
    if child.precedence() < parent.precedence() {
        format!("\\left( {} \\right)", child.to_tex())
    } else {
        child.to_tex()
    }
}

fn is_sqrt_exp(exp: &Expr) -> bool {
    match exp {
        Expr::Const(n) => *n == 0.5,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::*;

    #[test]
    fn to_tex_const_integer() {
        assert_eq!(constant(3.0).to_tex(), "3");
    }

    #[test]
    fn to_tex_const_float() {
        assert_eq!(constant(3.14).to_tex(), "3.14");
    }

    #[test]
    fn to_tex_scalar() {
        assert_eq!(scalar("x").to_tex(), "x");
    }

    #[test]
    fn to_tex_vector_upper() {
        let v = tensor("v", vec![upper("i")]);
        assert_eq!(v.to_tex(), "v^{i}");
    }

    #[test]
    fn to_tex_covector_lower() {
        let w = tensor("w", vec![lower("j")]);
        assert_eq!(w.to_tex(), "w_{j}");
    }

    #[test]
    fn to_tex_mixed_tensor() {
        let t = tensor("T", vec![upper("i"), lower("j"), lower("k")]);
        assert_eq!(t.to_tex(), "T^{i}_{jk}");
    }

    #[test]
    fn to_tex_add() {
        let e = add(scalar("x"), scalar("y"));
        assert_eq!(e.to_tex(), "x + y");
    }

    #[test]
    fn to_tex_subtract() {
        let e = add(scalar("x"), neg(scalar("y")));
        assert_eq!(e.to_tex(), "x - y");
    }

    #[test]
    fn to_tex_mul_scalar() {
        // Scalar multiplication (no contractions) - juxtaposition
        let e = mul(scalar("a"), scalar("b"));
        assert_eq!(e.to_tex(), "a b");
    }

    #[test]
    fn to_tex_mul_single_contraction() {
        // A^i B_i contracts on i → \cdot
        let e = mul(tensor("A", vec![upper("i")]), tensor("B", vec![lower("i")]));
        assert_eq!(e.to_tex(), "A^{i} \\cdot B_{i}");
    }

    #[test]
    fn to_tex_mul_double_contraction() {
        // A^{ij} B_{ij} contracts on both → :
        let e = mul(
            tensor("A", vec![upper("i"), upper("j")]),
            tensor("B", vec![lower("i"), lower("j")]),
        );
        assert_eq!(e.to_tex(), "A^{ij} : B_{ij}");
    }

    #[test]
    fn to_tex_mul_outer_product() {
        // A^i B^j has no contractions (both upper) - outer/tensor product
        let e = mul(tensor("A", vec![upper("i")]), tensor("B", vec![upper("j")]));
        assert_eq!(e.to_tex(), "A^{i} \\otimes B^{j}");
    }

    #[test]
    fn to_tex_div() {
        let e = mul(scalar("a"), inv(scalar("b")));
        assert_eq!(e.to_tex(), "\\frac{a}{b}");
    }

    #[test]
    fn to_tex_inv() {
        let e = inv(scalar("x"));
        assert_eq!(e.to_tex(), "\\frac{1}{x}");
    }

    #[test]
    fn to_tex_pow() {
        let e = pow(scalar("x"), constant(2.0));
        assert_eq!(e.to_tex(), "x^{2}");
    }

    #[test]
    fn to_tex_sqrt() {
        let e = sqrt(scalar("x"));
        assert_eq!(e.to_tex(), "\\sqrt{x}");
    }

    #[test]
    fn to_tex_sin() {
        let e = sin(scalar("x"));
        assert_eq!(e.to_tex(), "\\sin{x}");
    }

    #[test]
    fn to_tex_trig_inv() {
        assert_eq!(tan(scalar("x")).to_tex(), "\\tan{x}");
        assert_eq!(asin(scalar("x")).to_tex(), "\\arcsin{x}");
        assert_eq!(acos(scalar("x")).to_tex(), "\\arccos{x}");
        assert_eq!(atan(scalar("x")).to_tex(), "\\arctan{x}");
    }

    #[test]
    fn to_tex_log_base() {
        let e = mul(ln(scalar("x")), inv(ln(constant(10.0))));
        assert_eq!(e.to_tex(), "\\log_{10}{x}");
    }

    #[test]
    fn to_tex_nested_needs_parens() {
        // (x + y)^2
        let e = pow(add(scalar("x"), scalar("y")), constant(2.0));
        assert_eq!(e.to_tex(), "\\left( x + y \\right)^{2}");
    }

    #[test]
    fn to_tex_nested_no_parens() {
        // x * y^2 — no parens needed
        let e = mul(scalar("x"), pow(scalar("y"), constant(2.0)));
        assert_eq!(e.to_tex(), "x y^{2}");
    }

    #[test]
    fn to_tex_quadratic() {
        // ax^2 + bx + c
        let e = add(
            add(
                mul(scalar("a"), pow(scalar("x"), constant(2.0))),
                mul(scalar("b"), scalar("x")),
            ),
            scalar("c"),
        );
        assert_eq!(e.to_tex(), "a x^{2} + b x + c");
    }
}
