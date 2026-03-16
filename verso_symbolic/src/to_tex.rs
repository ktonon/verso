use crate::expr::{
    classify_mul, Expr, ExprKind, FnKind, Index, IndexPosition, MulKind, NamedConst,
};
use crate::rational::Rational;

fn frac_pi_to_tex(r: &Rational) -> String {
    let n = r.num();
    let d = r.den();
    match (n, d) {
        (0, _) => "0".to_string(),
        (1, 1) => "\\pi".to_string(),
        (-1, 1) => "-\\pi".to_string(),
        (_, 1) => format!("{}\\pi", n),
        (1, _) => format!("\\frac{{\\pi}}{{{}}}", d),
        (-1, _) => format!("-\\frac{{\\pi}}{{{}}}", d),
        _ if n < 0 => format!("-\\frac{{{}\\pi}}{{{}}}", -n, d),
        _ => format!("\\frac{{{}\\pi}}{{{}}}", n, d),
    }
}

pub trait ToTex {
    fn to_tex(&self) -> String;
}

impl ToTex for Index {
    fn to_tex(&self) -> String {
        self.name.clone()
    }
}

impl ToTex for NamedConst {
    fn to_tex(&self) -> String {
        match self {
            NamedConst::E => "e".to_string(),
            NamedConst::Sqrt2 => "\\sqrt{2}".to_string(),
            NamedConst::Sqrt3 => "\\sqrt{3}".to_string(),
            NamedConst::Frac1Sqrt2 => "\\frac{\\sqrt{2}}{2}".to_string(),
            NamedConst::FracSqrt3By2 => "\\frac{\\sqrt{3}}{2}".to_string(),
        }
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
            FnKind::Sign => "\\operatorname{sign}".to_string(),
            FnKind::Sinh => "\\sinh".to_string(),
            FnKind::Cosh => "\\cosh".to_string(),
            FnKind::Tanh => "\\tanh".to_string(),
            FnKind::Floor => "\\lfloor".to_string(),
            FnKind::Ceil => "\\lceil".to_string(),
            FnKind::Round => "\\operatorname{round}".to_string(),
            FnKind::Min => "\\min".to_string(),
            FnKind::Max => "\\max".to_string(),
            FnKind::Clamp => "\\operatorname{clamp}".to_string(),
            FnKind::Exp => "\\exp".to_string(),
            FnKind::Ln => "\\ln".to_string(),
            FnKind::Custom(name) => format!("\\operatorname{{{}}}", name),
        }
    }
}

impl ToTex for Expr {
    fn to_tex(&self) -> String {
        match &self.kind {
            ExprKind::Rational(r) => {
                if r.is_integer() {
                    format!("{}", r.num())
                } else if r.is_negative() {
                    format!("-\\frac{{{}}}{{{}}}", -r.num(), r.den())
                } else {
                    format!("\\frac{{{}}}{{{}}}", r.num(), r.den())
                }
            }
            ExprKind::Named(nc) => nc.to_tex(),
            ExprKind::FracPi(r) => frac_pi_to_tex(r),
            ExprKind::Var { name, indices, .. } => {
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
            ExprKind::Add(a, b) => {
                // Detect subtraction: Add(a, Neg(b)) -> a - b
                match &b.kind {
                    ExprKind::Neg(inner) => {
                        format!("{} - {}", a.to_tex(), inner.to_tex())
                    }
                    _ => {
                        format!("{} + {}", a.to_tex(), b.to_tex())
                    }
                }
            }
            ExprKind::Mul(a, b) => {
                if let Some((base, arg)) = match_log_base(a, b) {
                    return format!("\\log_{{{}}}{{{}}}", base.to_tex(), arg.to_tex());
                }

                // Detect division: Mul(a, Inv(b)) -> \frac{a}{b}
                match &b.kind {
                    ExprKind::Inv(inner) => {
                        format!("\\frac{{{}}}{{{}}}", a.to_tex(), inner.to_tex())
                    }
                    _ => {
                        let a_tex = maybe_paren(a, self);
                        let b_tex = maybe_paren(b, self);
                        // Use \times between numeric factors to avoid ambiguity (e.g. 2 \times 10^{10})
                        let op = if is_numeric_like(a) && is_numeric_like(b) {
                            " \\times "
                        } else {
                            match classify_mul(a, b) {
                                MulKind::Scalar => " ",         // scalar multiplication (juxtaposition)
                                MulKind::Outer => " \\otimes ", // outer/tensor product
                                MulKind::Single => " \\cdot ",  // single contraction (dot product)
                                MulKind::Double => " : ",       // double contraction
                            }
                        };
                        format!("{}{}{}", a_tex, op, b_tex)
                    }
                }
            }
            ExprKind::Neg(a) => {
                format!("-{}", maybe_paren(a, self))
            }
            ExprKind::Inv(a) => {
                format!("\\frac{{1}}{{{}}}", a.to_tex())
            }
            ExprKind::Pow(base, exp) => {
                if is_sqrt_exp(exp) {
                    return format!("\\sqrt{{{}}}", base.to_tex());
                }
                format!("{}^{{{}}}", maybe_paren(base, self), exp.to_tex())
            }
            ExprKind::Fn(kind, arg) => match kind {
                FnKind::Floor => format!("\\lfloor {} \\rfloor", arg.to_tex()),
                FnKind::Ceil => format!("\\lceil {} \\rceil", arg.to_tex()),
                _ => format!("{}{{{}}}", kind.to_tex(), arg.to_tex()),
            },
            ExprKind::FnN(kind, args) => {
                let rendered: Vec<String> = args.iter().map(|a| a.to_tex()).collect();
                let joined = rendered.join(", ");
                match kind {
                    FnKind::Min | FnKind::Max => {
                        format!("{}\\left( {} \\right)", kind.to_tex(), joined)
                    }
                    _ => format!("{}\\left( {} \\right)", kind.to_tex(), joined),
                }
            }
            ExprKind::Quantity(inner, unit) => {
                format!("{} \\; \\mathrm{{{}}}", inner.to_tex(), unit.display)
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

/// True when an expression is purely numeric (no variables), so that
/// adjacent numeric factors should be separated with `\times` in LaTeX.
fn is_numeric_like(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Rational(_) | ExprKind::Named(_) | ExprKind::FracPi(_) => true,
        ExprKind::Pow(base, _) => is_numeric_like(base),
        ExprKind::Neg(inner) => is_numeric_like(inner),
        ExprKind::Mul(a, b) => is_numeric_like(a) && is_numeric_like(b),
        _ => false,
    }
}

fn is_sqrt_exp(exp: &Expr) -> bool {
    match &exp.kind {
        ExprKind::Rational(r) => *r == Rational::new(1, 2),
        ExprKind::Inv(inner) => matches!(&inner.kind, ExprKind::Rational(r) if *r == Rational::TWO),
        _ => false,
    }
}

fn match_log_base<'a>(left: &'a Expr, right: &'a Expr) -> Option<(&'a Expr, &'a Expr)> {
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
        assert_eq!(constant(3.14).to_tex(), "\\frac{157}{50}");
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
    fn to_tex_misc_functions() {
        assert_eq!(sign(scalar("x")).to_tex(), "\\operatorname{sign}{x}");
        assert_eq!(sinh(scalar("x")).to_tex(), "\\sinh{x}");
        assert_eq!(cosh(scalar("x")).to_tex(), "\\cosh{x}");
        assert_eq!(tanh(scalar("x")).to_tex(), "\\tanh{x}");
        assert_eq!(floor(scalar("x")).to_tex(), "\\lfloor x \\rfloor");
        assert_eq!(ceil(scalar("x")).to_tex(), "\\lceil x \\rceil");
        assert_eq!(round(scalar("x")).to_tex(), "\\operatorname{round}{x}");
    }

    #[test]
    fn to_tex_multi_arg_functions() {
        assert_eq!(
            min(scalar("a"), scalar("b")).to_tex(),
            "\\min\\left( a, b \\right)"
        );
        assert_eq!(
            max(scalar("a"), scalar("b")).to_tex(),
            "\\max\\left( a, b \\right)"
        );
        assert_eq!(
            clamp(scalar("x"), constant(0.0), constant(1.0)).to_tex(),
            "\\operatorname{clamp}\\left( x, 0, 1 \\right)"
        );
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
    fn to_tex_numeric_mul_uses_times() {
        // 2 * 10^10 should use \times, not juxtaposition
        let e = mul(constant(2.0), pow(constant(10.0), constant(10.0)));
        assert_eq!(e.to_tex(), "2 \\times 10^{10}");
    }

    #[test]
    fn to_tex_numeric_mul_with_var() {
        // 2 * 10^10 * c — numeric part uses \times, var uses juxtaposition
        let e = mul(
            mul(constant(2.0), pow(constant(10.0), constant(10.0))),
            scalar("c"),
        );
        assert_eq!(e.to_tex(), "2 \\times 10^{10} c");
    }

    #[test]
    fn to_tex_frac_pi_negative_numerator() {
        // -3π/4
        let e = frac_pi(-3, 4);
        assert_eq!(e.to_tex(), "-\\frac{3\\pi}{4}");
    }

    #[test]
    fn to_tex_frac_pi_negative_one_over_d() {
        // -π/2
        let e = frac_pi(-1, 2);
        assert_eq!(e.to_tex(), "-\\frac{\\pi}{2}");
    }

    #[test]
    fn to_tex_frac_pi_integer_multiple() {
        // 3π
        let e = frac_pi(3, 1);
        assert_eq!(e.to_tex(), "3\\pi");
    }

    #[test]
    fn to_tex_frac_pi_zero() {
        let e = frac_pi(0, 1);
        assert_eq!(e.to_tex(), "0");
    }

    #[test]
    fn to_tex_negative_rational_fraction() {
        let e = rational(-3, 4);
        assert_eq!(e.to_tex(), "-\\frac{3}{4}");
    }

    #[test]
    fn to_tex_quantity() {
        use crate::dim::{BaseDim, Dimension};
        use crate::unit::Unit;
        let unit = Unit {
            dimension: Dimension::single(BaseDim::L, 1),
            scale: 1.0,
            display: "m".to_string(),
        };
        let e = quantity(constant(5.0), unit);
        assert_eq!(e.to_tex(), "5 \\; \\mathrm{m}");
    }

    #[test]
    fn to_tex_custom_fn() {
        let e = Expr::new(ExprKind::Fn(
            FnKind::Custom("foo".to_string()),
            Box::new(scalar("x")),
        ));
        assert_eq!(e.to_tex(), "\\operatorname{foo}{x}");
    }

    #[test]
    fn to_tex_neg_needs_parens() {
        // -(x + y)
        let e = neg(add(scalar("x"), scalar("y")));
        assert_eq!(e.to_tex(), "-\\left( x + y \\right)");
    }

    #[test]
    fn to_tex_log_base_reversed() {
        // ln(base)^-1 * ln(arg) — reversed order
        let e = mul(inv(ln(constant(2.0))), ln(scalar("x")));
        assert_eq!(e.to_tex(), "\\log_{2}{x}");
    }

    #[test]
    fn to_tex_sqrt_via_inv() {
        // x^(1/2) expressed as Pow(x, Inv(2))
        let e = Expr::new(ExprKind::Pow(
            Box::new(scalar("x")),
            Box::new(Expr::new(ExprKind::Inv(Box::new(constant(2.0))))),
        ));
        assert_eq!(e.to_tex(), "\\sqrt{x}");
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
