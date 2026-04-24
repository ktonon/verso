use crate::expr::{
    classify_mul, match_log_base, Expr, ExprKind, FnKind, Index, IndexPosition, MulKind,
    NamedConst,
};
use crate::rational::Rational;
use std::fmt::Display;

/// Format a rational multiple of π for display.
pub fn fmt_frac_pi(r: &Rational) -> String {
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

/// Color theme for expression formatting.
/// For plain text, all fields are empty strings.
struct Theme {
    reset: &'static str,
    dim: &'static str,
    cyan: &'static str,
    magenta: &'static str,
    yellow: &'static str,
    bold_yellow: &'static str,
    blue: &'static str,
}

const PLAIN: Theme = Theme {
    reset: "",
    dim: "",
    cyan: "",
    magenta: "",
    yellow: "",
    bold_yellow: "",
    blue: "",
};

const COLORED: Theme = Theme {
    reset: "\x1b[0m",
    dim: "\x1b[2m",
    cyan: "\x1b[36m",
    magenta: "\x1b[35m",
    yellow: "\x1b[33m",
    bold_yellow: "\x1b[1;33m",
    blue: "\x1b[34m",
};

/// Wrapper for colored display of expressions
pub struct Colored<'a>(pub &'a Expr);

impl<'a> Display for Colored<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", fmt_colored(self.0))
    }
}

/// Format an expression with ANSI colors
pub fn fmt_colored(expr: &Expr) -> String {
    format_expr(expr, &COLORED)
}

fn format_expr(expr: &Expr, t: &Theme) -> String {
    match &expr.kind {
        ExprKind::Rational(r) => {
            format!("{}{}{}", t.cyan, r, t.reset)
        }
        ExprKind::Named(nc) => {
            format!("{}{}{}", t.magenta, nc, t.reset)
        }
        ExprKind::FracPi(r) => {
            format!("{}{}{}", t.magenta, fmt_frac_pi(r), t.reset)
        }
        ExprKind::Var { name, indices, dim } => {
            let order = indices.len();
            let var_color = if order >= 2 { t.bold_yellow } else { t.yellow };

            let mut result = if indices.is_empty() {
                format!("{}{}{}", var_color, name, t.reset)
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

                let mut r = format!("{}{}{}", var_color, name, t.reset);
                if !lower.is_empty() {
                    r.push_str(&format!("{}_{{{}}}{}", t.dim, lower.join(","), t.reset));
                }
                if !upper.is_empty() {
                    r.push_str(&format!("{}^{{{}}}{}", t.dim, upper.join(","), t.reset));
                }
                r
            };
            if let Some(d) = dim {
                result.push_str(&format!(" {}{}{}", t.dim, d, t.reset));
            }
            result
        }
        ExprKind::Add(a, b) => match &b.kind {
            ExprKind::Neg(inner) => {
                format!(
                    "{}{} - {}{}",
                    format_expr(a, t),
                    t.dim,
                    t.reset,
                    format_expr(inner, t)
                )
            }
            ExprKind::Rational(r) if r.is_negative() => {
                format!(
                    "{}{} - {}{}{}{}",
                    format_expr(a, t),
                    t.dim,
                    t.reset,
                    t.cyan,
                    -(*r),
                    t.reset
                )
            }
            _ => {
                format!(
                    "{}{} + {}{}",
                    format_expr(a, t),
                    t.dim,
                    t.reset,
                    format_expr(b, t)
                )
            }
        },
        ExprKind::Mul(a, b) => {
            if let Some((base, arg)) = match_log_base(a, b) {
                return format!(
                    "{}log{}_{}{}{}({}){}",
                    t.blue,
                    t.reset,
                    t.dim,
                    format_log_base(base, t),
                    t.blue,
                    format_expr(arg, t),
                    t.reset
                );
            }

            match &b.kind {
                ExprKind::Inv(inner) => {
                    format!(
                        "{}{}/{}{}",
                        format_paren(a, expr, t),
                        t.dim,
                        t.reset,
                        format_paren(inner, expr, t)
                    )
                }
                _ => {
                    let mul_kind = classify_mul(a, b);
                    if mul_kind == MulKind::Scalar {
                        if let ExprKind::Rational(r) = &a.kind {
                            return format!(
                                "{}{}{}{}",
                                t.cyan,
                                r,
                                t.reset,
                                format_paren(b, expr, t)
                            );
                        }
                        if let ExprKind::Rational(r) = &b.kind {
                            return format!(
                                "{}{}{}{}",
                                t.cyan,
                                r,
                                t.reset,
                                format_paren(a, expr, t)
                            );
                        }
                    }
                    let op_char = match mul_kind {
                        MulKind::Scalar => "⋅",
                        MulKind::Outer => "⊗",
                        MulKind::Single => "⋅",
                        MulKind::Double => ":",
                    };
                    format!(
                        "{}{}{}{}{}",
                        format_paren(a, expr, t),
                        t.dim,
                        op_char,
                        t.reset,
                        format_paren(b, expr, t)
                    )
                }
            }
        }
        ExprKind::Neg(a) => {
            format!("{}-{}{}", t.dim, t.reset, format_paren(a, expr, t))
        }
        ExprKind::Inv(a) => {
            format!("{}1/{}{}", t.dim, t.reset, format_paren(a, expr, t))
        }
        ExprKind::Pow(base, exp) => {
            if exp.is_sqrt_exp() {
                return format!(
                    "{}sqrt({}){}",
                    t.blue,
                    format_expr(base, t),
                    t.reset
                );
            }
            format!(
                "{}{}^{}{}",
                format_paren(base, expr, t),
                t.dim,
                t.reset,
                format_paren(exp, expr, t)
            )
        }
        ExprKind::Fn(kind, arg) => {
            format!(
                "{}{}{}({})",
                t.blue,
                kind,
                t.reset,
                format_expr(arg, t)
            )
        }
        ExprKind::FnN(kind, args) => {
            let rendered: Vec<String> = args.iter().map(|a| format_expr(a, t)).collect();
            format!(
                "{}{}{}({})",
                t.blue,
                kind,
                t.reset,
                rendered.join(", ")
            )
        }
        ExprKind::Quantity(inner, unit) => {
            format!(
                "{} {}[{}]{}",
                format_expr(inner, t),
                t.dim,
                unit,
                t.reset
            )
        }
    }
}

fn format_paren(child: &Expr, parent: &Expr, t: &Theme) -> String {
    if child.precedence() < parent.precedence() {
        format!(
            "{}({}{}){}",
            t.dim,
            t.reset,
            format_expr(child, t),
            t.dim
        )
    } else {
        format_expr(child, t)
    }
}

fn format_log_base(base: &Expr, t: &Theme) -> String {
    match &base.kind {
        ExprKind::Rational(_) | ExprKind::Var { .. } => format_expr(base, t),
        _ => format!(
            "{}({}{}){}",
            t.dim,
            t.reset,
            format_expr(base, t),
            t.dim
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
            FnKind::Custom(name) => write!(f, "{}", name),
        }
    }
}

impl std::fmt::Display for Expr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", format_expr(self, &PLAIN))
    }
}

#[cfg(test)]
mod tests {
    use crate::expr::*;

    #[test]
    fn display_const() {
        assert_eq!(format!("{}", constant(3.0)), "3");
        assert_eq!(format!("{}", constant(3.5)), "7/2");
        assert_eq!(format!("{}", constant(-3.5)), "-7/2");
    }

    #[test]
    fn display_scalar() {
        assert_eq!(format!("{}", scalar("x")), "x");
    }

    #[test]
    fn display_tensor() {
        assert_eq!(format!("{}", tensor("X", vec![lower("i")])), "X_{i}");
        assert_eq!(format!("{}", tensor("X", vec![upper("i")])), "X^{i}");
        assert_eq!(
            format!("{}", tensor("X", vec![lower("i"), lower("j"), upper("k")])),
            "X_{i,j}^{k}"
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
        assert_eq!(format!("{}", mul(scalar("x"), scalar("y"))), "x⋅y");
        assert_eq!(format!("{}", mul(constant(2.0), scalar("y"))), "2y");
        // Coefficient on right still displays coefficient-first
        assert_eq!(format!("{}", mul(scalar("x"), constant(2.0))), "2x");
    }

    #[test]
    fn display_mul_single_contraction() {
        let e = mul(tensor("A", vec![upper("i")]), tensor("B", vec![lower("i")]));
        assert_eq!(format!("{}", e), "A^{i}⋅B_{i}");
    }

    #[test]
    fn display_mul_double_contraction() {
        let e = mul(
            tensor("A", vec![upper("i"), upper("j")]),
            tensor("B", vec![lower("i"), lower("j")]),
        );
        assert_eq!(format!("{}", e), "A^{i,j}:B_{i,j}");
    }

    #[test]
    fn display_mul_outer_product() {
        let e = mul(tensor("A", vec![upper("i")]), tensor("B", vec![upper("j")]));
        assert_eq!(format!("{}", e), "A^{i}⊗B^{j}");
    }

    #[test]
    fn display_div() {
        assert_eq!(format!("{}", div(scalar("x"), scalar("y"))), "x/y");
    }

    #[test]
    fn display_neg() {
        assert_eq!(
            format!("{}", neg(add(constant(2.5), scalar("x")))),
            "-(5/2 + x)"
        );
        assert_eq!(format!("{}", neg(scalar("x"))), "-x");
    }

    #[test]
    fn display_inv() {
        assert_eq!(format!("{}", inv(scalar("x"))), "1/x");
        assert_eq!(
            format!("{}", inv(add(constant(2.5), scalar("x")))),
            "1/(5/2 + x)"
        );
    }

    #[test]
    fn display_pow() {
        assert_eq!(format!("{}", pow(scalar("x"), constant(2.0))), "x^2");
        assert_eq!(
            format!("{}", pow(scalar("y"), add(constant(2.0), scalar("x")))),
            "y^(2 + x)"
        );
        assert_eq!(
            format!("{}", pow(add(constant(2.0), scalar("x")), scalar("y"))),
            "(2 + x)^y"
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
    fn display_var_with_dimension() {
        use crate::dim::Dimension;
        let e = scalar_dim("v", Dimension::parse("[L T^-1]").unwrap());
        assert_eq!(format!("{}", e), "v [L T^-1]");
    }

    #[test]
    fn display_quantity() {
        use crate::dim::Dimension;
        use crate::unit::Unit;
        let km = Unit {
            dimension: Dimension::parse("[L]").unwrap(),
            scale: 1000.0,
            display: "km".to_string(),
        };
        let e = quantity(constant(1.0), km);
        assert_eq!(format!("{}", e), "1 [km]");
    }

    #[test]
    fn collect_units_from_quantity() {
        use crate::dim::Dimension;
        use crate::unit::Unit;
        let km = Unit {
            dimension: Dimension::parse("[L]").unwrap(),
            scale: 1000.0,
            display: "km".to_string(),
        };
        let e = quantity(constant(1.0), km);
        assert_eq!(e.collect_units(), vec!["km"]);
    }

    #[test]
    fn collect_units_empty_for_scalar() {
        let e = scalar("x");
        assert!(e.collect_units().is_empty());
    }

    #[test]
    fn display_nested() {
        let e = add(
            add(
                pow(scalar("x"), constant(2.0)),
                mul(constant(2.0), scalar("x")),
            ),
            constant(1.0),
        );
        assert_eq!(format!("{}", e), "x^2 + 2x + 1");
    }

    // --- fmt_frac_pi tests ---

    use super::{fmt_colored, fmt_frac_pi, Colored};
    use crate::rational::Rational;

    #[test]
    fn frac_pi_zero() {
        assert_eq!(fmt_frac_pi(&Rational::new(0, 1)), "0");
    }

    #[test]
    fn frac_pi_one_pi() {
        assert_eq!(fmt_frac_pi(&Rational::new(1, 1)), "π");
    }

    #[test]
    fn frac_pi_neg_pi() {
        assert_eq!(fmt_frac_pi(&Rational::new(-1, 1)), "-π");
    }

    #[test]
    fn frac_pi_integer_multiple() {
        assert_eq!(fmt_frac_pi(&Rational::new(3, 1)), "3π");
    }

    #[test]
    fn frac_pi_over_d() {
        assert_eq!(fmt_frac_pi(&Rational::new(1, 4)), "π/4");
    }

    #[test]
    fn frac_pi_neg_over_d() {
        assert_eq!(fmt_frac_pi(&Rational::new(-1, 4)), "-π/4");
    }

    #[test]
    fn frac_pi_general() {
        assert_eq!(fmt_frac_pi(&Rational::new(3, 4)), "3π/4");
    }

    // --- fmt_colored tests ---

    #[test]
    fn colored_rational() {
        let s = fmt_colored(&constant(3.0));
        assert!(s.contains("3"));
        assert!(s.contains("\x1b["));
    }

    #[test]
    fn colored_named_const() {
        let s = fmt_colored(&Expr::new(ExprKind::Named(NamedConst::E)));
        assert!(s.contains("e"));
    }

    #[test]
    fn colored_frac_pi() {
        let s = fmt_colored(&Expr::new(ExprKind::FracPi(Rational::new(1, 2))));
        assert!(s.contains("π/2"));
    }

    #[test]
    fn colored_scalar() {
        let s = fmt_colored(&scalar("x"));
        assert!(s.contains("x"));
    }

    #[test]
    fn colored_tensor_lower() {
        let s = fmt_colored(&tensor("A", vec![lower("i")]));
        assert!(s.contains("A"));
        assert!(s.contains("i"));
    }

    #[test]
    fn colored_tensor_upper() {
        let s = fmt_colored(&tensor("A", vec![upper("i")]));
        assert!(s.contains("A"));
    }

    #[test]
    fn colored_tensor_mixed() {
        let s = fmt_colored(&tensor("T", vec![lower("i"), upper("j")]));
        assert!(s.contains("T"));
    }

    #[test]
    fn colored_var_with_dim() {
        use crate::dim::Dimension;
        let s = fmt_colored(&scalar_dim("v", Dimension::parse("[L]").unwrap()));
        assert!(s.contains("v"));
    }

    #[test]
    fn colored_add() {
        let s = fmt_colored(&add(scalar("x"), constant(2.0)));
        assert!(s.contains("x"));
        assert!(s.contains("2"));
    }

    #[test]
    fn colored_add_neg_as_sub() {
        let s = fmt_colored(&add(scalar("x"), neg(scalar("y"))));
        assert!(s.contains("-"));
    }

    #[test]
    fn colored_add_negative_rational() {
        let s = fmt_colored(&add(scalar("x"), constant(-2.0)));
        assert!(s.contains("-"));
        assert!(s.contains("2"));
    }

    #[test]
    fn colored_mul_coefficient() {
        let s = fmt_colored(&mul(constant(2.0), scalar("x")));
        assert!(s.contains("2"));
        assert!(s.contains("x"));
    }

    #[test]
    fn colored_mul_scalars() {
        let s = fmt_colored(&mul(scalar("x"), scalar("y")));
        assert!(s.contains("x"));
        assert!(s.contains("y"));
    }

    #[test]
    fn colored_neg() {
        let s = fmt_colored(&neg(scalar("x")));
        assert!(s.contains("-"));
        assert!(s.contains("x"));
    }

    #[test]
    fn colored_inv() {
        let s = fmt_colored(&inv(scalar("x")));
        assert!(s.contains("1"));
        assert!(s.contains("x"));
    }

    #[test]
    fn colored_pow() {
        let s = fmt_colored(&pow(scalar("x"), constant(2.0)));
        assert!(s.contains("x"));
        assert!(s.contains("2"));
    }

    #[test]
    fn colored_sqrt() {
        let s = fmt_colored(&sqrt(scalar("x")));
        assert!(s.contains("sqrt"));
    }

    #[test]
    fn colored_fn_sin() {
        let s = fmt_colored(&sin(scalar("x")));
        assert!(s.contains("sin"));
    }

    #[test]
    fn colored_fn_multi_arg() {
        let s = fmt_colored(&min(scalar("a"), scalar("b")));
        assert!(s.contains("min"));
    }

    #[test]
    fn colored_quantity() {
        use crate::dim::Dimension;
        use crate::unit::Unit;
        let m = Unit {
            dimension: Dimension::parse("[L]").unwrap(),
            scale: 1.0,
            display: "m".to_string(),
        };
        let s = fmt_colored(&quantity(constant(5.0), m));
        assert!(s.contains("5"));
        assert!(s.contains("m"));
    }

    #[test]
    fn colored_log_base() {
        let e = mul(ln(scalar("x")), inv(ln(constant(10.0))));
        let s = fmt_colored(&e);
        assert!(s.contains("log"));
        assert!(s.contains("10"));
    }

    #[test]
    fn colored_wrapper_display() {
        let e = constant(42.0);
        let s = format!("{}", Colored(&e));
        assert!(s.contains("42"));
    }

    #[test]
    fn colored_div() {
        let s = fmt_colored(&div(scalar("x"), scalar("y")));
        assert!(s.contains("x"));
        assert!(s.contains("y"));
    }
}
