use crate::context::{format_dim_error, Context, DimOutcome, EqualityResult};
use crate::dim::Dimension;
use crate::eval::free_vars;
use crate::expr::{Expr, ExprKind, Ty};
use crate::fmt::fmt_colored;
use crate::parser::parse_expr;
use crate::search;
use crate::unit::base_si_display;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum HistoryMode {
    Results,
    Inputs,
}

pub fn run() -> Result<(), ReadlineError> {
    let mut rl = DefaultEditor::new()?;
    let mut ctx = Context::new();
    let mut show_trace = false;
    let mut history_mode = HistoryMode::Inputs;
    let mut input_history: Vec<String> = Vec::new();
    let mut result_history: Vec<String> = Vec::new();
    let mut claim_counter: usize = 0;

    loop {
        match rl.readline("> ") {
            Ok(line) => {
                let input = line.trim();
                if input.is_empty() {
                    continue;
                }
                if input == ":q" || input == ":quit" || input == ":exit" {
                    break;
                }
                if input == ":trace" {
                    show_trace = !show_trace;
                    println!("trace: {}\n", if show_trace { "on" } else { "off" });
                    continue;
                }
                if input == ":history" || input == ":hist" {
                    history_mode = match history_mode {
                        HistoryMode::Results => HistoryMode::Inputs,
                        HistoryMode::Inputs => HistoryMode::Results,
                    };
                    match history_mode {
                        HistoryMode::Results => {
                            reload_history(&mut rl, &result_history);
                            println!("history: results\n");
                        }
                        HistoryMode::Inputs => {
                            reload_history(&mut rl, &input_history);
                            println!("history: inputs\n");
                        }
                    }
                    continue;
                }
                if input == ":reset" {
                    ctx = Context::new();
                    claim_counter = 0;
                    println!("context reset\n");
                    continue;
                }

                // :var declaration
                if input.starts_with(":var") {
                    let rest = input[":var".len()..].trim();
                    match parse_var_decl(rest) {
                        Ok((name, dim)) => {
                            ctx.declare_var(&name, Some(dim.clone()));
                            println!("\x1b[90m{}: {}\x1b[0m\n", name, dim);
                        }
                        Err(msg) => println!("Error: {}\n", msg),
                    }
                    continue;
                }

                // :const declaration
                if input.starts_with(":const") {
                    let rest = input[":const".len()..].trim();
                    match parse_const_decl(rest) {
                        Ok((name, value)) => {
                            let (stripped, inline_dims) = ctx.push_inline_dims(&value);

                            if let Some(Err(e)) = ctx.check_expr_dim(&stripped) {
                                let value_str = rest[rest.find('=').unwrap() + 1..].trim();
                                let value_offset =
                                    input.chars().count() - value_str.chars().count();
                                println!("{}", format_dim_error(&e, value_str, 2 + value_offset));
                            }
                            let simplified = ctx.simplify(&stripped);
                            let expr_ty = ctx.infer_ty(&simplified);
                            let type_suffix =
                                format_type_suffix(&simplified, expr_ty.as_ref());
                            println!(
                                "\x1b[90m{} = {}{}\x1b[0m\n",
                                name,
                                fmt_colored(&simplified),
                                type_suffix
                            );
                            ctx.declare_const(&name, value);

                            ctx.pop_dims(&inline_dims);
                        }
                        Err(msg) => println!("Error: {}\n", msg),
                    }
                    continue;
                }

                // :func declaration
                if input.starts_with(":func") {
                    let rest = input[":func".len()..].trim();
                    match parse_func_decl(rest) {
                        Ok((name, params, body)) => {
                            println!(
                                "\x1b[90m{}({}) = {}\x1b[0m\n",
                                name,
                                params.join(", "),
                                fmt_colored(&body)
                            );
                            ctx.declare_func(&name, params, body);
                        }
                        Err(msg) => println!("Error: {}\n", msg),
                    }
                    continue;
                }

                record_input(&mut input_history, &mut rl, history_mode, input);

                if let Some(eq_pos) = input.find('=') {
                    let lhs_str = input[..eq_pos].trim();
                    let rhs_str = input[eq_pos + 1..].trim();
                    match (parse_expr(lhs_str), parse_expr(rhs_str)) {
                        (Ok(lhs), Ok(rhs)) => {
                            let (lhs, lhs_dims) = ctx.push_inline_dims(&lhs);
                            let (rhs, rhs_dims) = ctx.push_inline_dims(&rhs);

                            // Dimensional check on equality
                            match ctx.check_dims(&lhs, &rhs) {
                                DimOutcome::Pass => {}
                                DimOutcome::Skipped { .. } => {}
                                DimOutcome::LhsRhsMismatch { lhs: dl, rhs: dr } => {
                                    println!(
                                        "\x1b[31mdim error: lhs is {}, rhs is {}\x1b[0m",
                                        dl, dr
                                    );
                                }
                                DimOutcome::ExprError { side, error } => {
                                    let (source, offset) = if side == "lhs" {
                                        (lhs_str, 0)
                                    } else {
                                        (rhs_str, input.chars().count() - rhs_str.chars().count())
                                    };
                                    println!("{}", format_dim_error(&error, source, 2 + offset));
                                }
                            }
                            let result = ctx.check_equal(&lhs, &rhs);
                            match &result {
                                EqualityResult::Equal => {
                                    println!("\x1b[32mtrue\x1b[0m\n");
                                }
                                EqualityResult::NumericallyEqual { .. } => {
                                    println!("\x1b[32mtrue\x1b[0m (numerical)\n");
                                }
                                EqualityResult::NotEqual { residual } => {
                                    println!(
                                        "\x1b[31mfalse\x1b[0m  residual: {}\n",
                                        fmt_colored(residual)
                                    );
                                }
                            }
                            if result.passed() {
                                claim_counter += 1;
                                ctx.add_claim_as_rule(
                                    &format!("repl_{}", claim_counter),
                                    &lhs,
                                    &rhs,
                                );
                            }
                            let diff = Expr::new(ExprKind::Add(
                                Box::new(lhs),
                                Box::new(Expr::new(ExprKind::Neg(Box::new(rhs)))),
                            ));
                            let simplified = ctx.simplify(&diff);
                            record_result(&mut result_history, &mut rl, history_mode, &simplified);

                            ctx.pop_dims(&lhs_dims);
                            ctx.pop_dims(&rhs_dims);
                        }
                        (Err(err), _) | (_, Err(err)) => {
                            println!("Error: {:?}\n", err);
                        }
                    }
                } else {
                    match parse_expr(input) {
                        Ok(expr) => {
                            // Absorb inline dim annotations into a transient scope
                            let (expr, inline_dims) = ctx.push_inline_dims(&expr);

                            // Dimensional consistency check
                            if let Some(Err(e)) = ctx.check_expr_dim(&expr) {
                                println!("{}", format_dim_error(&e, input, 2));
                            }
                            if show_trace {
                                let applied = ctx.apply_consts(&expr);
                                let (simplified, trace) =
                                    search::simplify_with_trace(&applied, &ctx.rules);

                                let plain_widths: Vec<usize> = trace
                                    .iter()
                                    .map(|s| format!("{}", s.expr).chars().count())
                                    .collect();
                                let max_expr_width =
                                    plain_widths.iter().copied().max().unwrap_or(0);
                                let max_name_width = trace
                                    .iter()
                                    .filter_map(|s| s.rule_name.as_ref().map(|n| n.len()))
                                    .max()
                                    .unwrap_or(0);

                                for (i, step) in trace.iter().enumerate() {
                                    let expr_str = fmt_colored(&step.expr);
                                    let padding = max_expr_width - plain_widths[i];
                                    match (&step.rule_name, &step.rule_display) {
                                        (Some(name), Some(display)) => {
                                            println!(
                                                "{}: {}{:padding$}  \x1b[90m{:width$}\x1b[0m  \x1b[2m{}\x1b[0m",
                                                i,
                                                expr_str,
                                                "",
                                                name,
                                                display,
                                                padding = padding,
                                                width = max_name_width,
                                            );
                                        }
                                        _ => println!("{}: {}", i, expr_str),
                                    }
                                }
                                println!();
                                record_result(
                                    &mut result_history,
                                    &mut rl,
                                    history_mode,
                                    &simplified,
                                );
                            } else {
                                let simplified = ctx.simplify(&expr);
                                let expr_ty = ctx.infer_ty(&simplified);
                                let type_suffix =
                                    format_type_suffix(&simplified, expr_ty.as_ref());
                                println!("{}{}\n", fmt_colored(&simplified), type_suffix);
                                record_result(
                                    &mut result_history,
                                    &mut rl,
                                    history_mode,
                                    &simplified,
                                );
                            }

                            ctx.pop_dims(&inline_dims);
                        }
                        Err(err) => {
                            println!("Error: {:?}\n", err);
                        }
                    }
                }
            }
            Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => {
                break;
            }
            Err(_) => {
                break;
            }
        }
    }

    Ok(())
}

fn parse_var_decl(rest: &str) -> Result<(String, Dimension), String> {
    let bracket_pos = rest
        .find('[')
        .ok_or(":var requires name [dims], e.g. :var v [L T^-1]")?;
    let name = rest[..bracket_pos].trim().to_string();
    if name.is_empty() {
        return Err(":var requires a variable name".into());
    }
    let dim_str = rest[bracket_pos..].trim();
    let dimension = Dimension::parse(dim_str).map_err(|e| format!("{}", e))?;
    Ok((name, dimension))
}

fn parse_const_decl(rest: &str) -> Result<(String, Expr), String> {
    let eq_pos = rest
        .find('=')
        .ok_or(":const requires name = expr, e.g. :const c = 3*10^8")?;
    let name = rest[..eq_pos].trim().to_string();
    if name.is_empty() {
        return Err(":const requires a name".into());
    }
    let value_str = rest[eq_pos + 1..].trim();
    let value = parse_expr(value_str).map_err(|e| format!("{:?}", e))?;
    Ok((name, value))
}

fn parse_func_decl(rest: &str) -> Result<(String, Vec<String>, Expr), String> {
    let lparen = rest.find('(').ok_or(":func requires name(params) = expr")?;
    let name = rest[..lparen].trim().to_string();
    if name.is_empty() {
        return Err(":func requires a name".into());
    }
    let rparen = rest.find(')').ok_or(":func missing closing parenthesis")?;
    let params: Vec<String> = rest[lparen + 1..rparen]
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if params.is_empty() {
        return Err(":func requires at least one parameter".into());
    }
    let after = rest[rparen + 1..].trim();
    let body_str = after
        .strip_prefix('=')
        .ok_or(":func requires = after parameters")?
        .trim();
    let body = parse_expr(body_str).map_err(|e| format!("{:?}", e))?;
    Ok((name, params, body))
}

fn record_input(
    input_history: &mut Vec<String>,
    rl: &mut DefaultEditor,
    history_mode: HistoryMode,
    input: &str,
) {
    input_history.push(input.to_string());
    if history_mode == HistoryMode::Inputs {
        let _ = rl.add_history_entry(input);
    }
}

fn record_result(
    result_history: &mut Vec<String>,
    rl: &mut DefaultEditor,
    history_mode: HistoryMode,
    simplified: &Expr,
) {
    let rendered = format!("{}", simplified);
    result_history.push(rendered.clone());
    if history_mode == HistoryMode::Results {
        let _ = rl.add_history_entry(rendered);
    }
}

/// Format the type suffix for a REPL result.
/// - Symbolic results (containing variables) → dimension notation: `[L]`, `[M L T^-2]`
/// - Numeric results → unit notation: `[m]`, `[N]`
/// - Unresolved internal results → `"[?]"`
/// - Results already displaying units (Quantity nodes) → no suffix needed
fn format_type_suffix(simplified: &Expr, ty: Option<&Ty>) -> String {
    let ty = match ty {
        Some(ty) => ty,
        None => return String::new(),
    };
    if simplified.first_unit().is_some() {
        return String::new();
    }
    match ty {
        Ty::Concrete(dim) => {
            if free_vars(simplified).is_empty() {
                format!(" \x1b[2m[{}]\x1b[0m", base_si_display(dim))
            } else {
                format!(" \x1b[2m{}\x1b[0m", dim)
            }
        }
        Ty::Unresolved => " \x1b[33m[?]\x1b[0m".to_string(),
    }
}

fn reload_history(rl: &mut DefaultEditor, entries: &[String]) {
    let _ = rl.clear_history();
    for entry in entries {
        let _ = rl.add_history_entry(entry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_type_suffix_shows_dimensionless_numeric_type() {
        let suffix = format_type_suffix(
            &Expr::new(ExprKind::Rational(crate::rational::Rational::from_i64(4))),
            Some(&Ty::Concrete(Dimension::dimensionless())),
        );
        assert!(suffix.contains("[1]"));
        assert!(suffix.contains("\x1b[2m"));
    }

    #[test]
    fn format_type_suffix_shows_dimensionless_symbol_type() {
        let suffix = format_type_suffix(
            &crate::expr::scalar("x"),
            Some(&Ty::Concrete(Dimension::dimensionless())),
        );
        assert!(suffix.contains("[1]"));
        assert!(suffix.contains("\x1b[2m"));
    }

    #[test]
    fn bare_symbol_repl_type_suffix_is_dimensionless() {
        let ctx = Context::new();
        let expr = parse_expr("x").unwrap();
        let ty = ctx.infer_ty(&expr);
        let suffix = format_type_suffix(&expr, ty.as_ref());
        assert!(suffix.contains("[1]"));
    }

    #[test]
    fn inline_dim_shows_combined_type_suffix() {
        let mut ctx = Context::new();
        let expr = parse_expr("a [L]").unwrap();
        let (stripped, inline_dims) = ctx.push_inline_dims(&expr);
        let ty = ctx.infer_ty(&stripped);
        let suffix = format_type_suffix(&stripped, ty.as_ref());
        assert!(suffix.contains("[L]"));
        ctx.pop_dims(&inline_dims);
    }

    #[test]
    fn inline_dims_combine_through_division() {
        let mut ctx = Context::new();
        let expr = parse_expr("a [L] / b [T]").unwrap();
        let (stripped, inline_dims) = ctx.push_inline_dims(&expr);
        // Stripped expression has no inline dim annotations
        assert_eq!(format!("{}", stripped), "a/b");
        // Combined type is L T^-1
        let ty = ctx.infer_ty(&stripped);
        let suffix = format_type_suffix(&stripped, ty.as_ref());
        assert!(suffix.contains("[L T^-1]"), "got: {}", suffix);
        ctx.pop_dims(&inline_dims);
    }

    #[test]
    fn inline_dims_are_transient() {
        let mut ctx = Context::new();
        let expr = parse_expr("a [L]").unwrap();
        let (_, inline_dims) = ctx.push_inline_dims(&expr);
        assert!(ctx.dims.contains_key("a"));
        ctx.pop_dims(&inline_dims);
        assert!(!ctx.dims.contains_key("a"));
    }
}
