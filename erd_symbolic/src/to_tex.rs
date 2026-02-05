use crate::expr::{Expr, FnKind, Index, IndexPosition};

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
                // Detect division: Mul(a, Inv(b)) -> \frac{a}{b}
                match b.as_ref() {
                    Expr::Inv(inner) => {
                        format!("\\frac{{{}}}{{{}}}", a.to_tex(), inner.to_tex())
                    }
                    _ => {
                        let a_tex = maybe_paren(a, self);
                        let b_tex = maybe_paren(b, self);
                        format!("{} {}", a_tex, b_tex)
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
    fn to_tex_mul() {
        let e = mul(scalar("a"), scalar("b"));
        assert_eq!(e.to_tex(), "a b");
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
    fn to_tex_sin() {
        let e = sin(scalar("x"));
        assert_eq!(e.to_tex(), "\\sin{x}");
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
