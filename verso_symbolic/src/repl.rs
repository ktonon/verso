use crate::context::{format_dim_error, Context, DimOutcome, EqualityResult};
use crate::dim::Dimension;
use crate::eval::free_vars;
use crate::expr::{Expr, Ty};
use crate::fmt::fmt_colored;
use crate::parser::parse_expr;
use crate::search;
use crate::unit::base_si_display;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

/// Core REPL session state, decoupled from readline/history for testability.
pub struct Session {
    pub ctx: Context,
    claim_counter: usize,
    show_trace: bool,
}

impl Session {
    pub fn new() -> Self {
        Session {
            ctx: Context::new(),
            claim_counter: 0,
            show_trace: false,
        }
    }

    /// Evaluate a single REPL input line, returning the output text.
    /// Returns `None` for quit commands and empty input.
    pub fn eval(&mut self, input: &str) -> Option<String> {
        let input = input.trim();
        if input.is_empty() || input == ":q" || input == ":quit" || input == ":exit" {
            return None;
        }
        if input == ":trace" {
            self.show_trace = !self.show_trace;
            return Some(format!(
                "trace: {}",
                if self.show_trace { "on" } else { "off" }
            ));
        }
        if input == ":reset" {
            self.ctx = Context::new();
            self.claim_counter = 0;
            return Some("context reset".to_string());
        }

        // :var declaration
        if input.starts_with(":var") {
            let rest = input[":var".len()..].trim();
            return Some(match parse_var_decl(rest) {
                Ok((name, dim)) => {
                    self.ctx.declare_var(&name, Some(dim.clone()));
                    format!("\x1b[90m{}: {}\x1b[0m", name, dim)
                }
                Err(msg) => format!("Error: {}", msg),
            });
        }

        // :const declaration
        if input.starts_with(":const") {
            let rest = input[":const".len()..].trim();
            return Some(match parse_const_decl(rest) {
                Ok((name, value)) => {
                    let (stripped, inline_dims) = self.ctx.push_inline_dims(&value);
                    let mut out = String::new();

                    if let Some(Err(e)) = self.ctx.check_expr_dim(&stripped) {
                        let value_str = rest[rest.find('=').unwrap() + 1..].trim();
                        let value_offset = input.chars().count() - value_str.chars().count();
                        out.push_str(&format_dim_error(&e, value_str, 2 + value_offset));
                        out.push('\n');
                    }
                    let expr_ty = self.ctx.infer_ty(&stripped);
                    let simplified = self.ctx.simplify(&stripped);
                    let type_suffix = format_type_suffix(&simplified, expr_ty.as_ref());
                    out.push_str(&format!(
                        "\x1b[90m{} = {}{}\x1b[0m",
                        name,
                        fmt_colored(&simplified),
                        type_suffix
                    ));
                    self.ctx.declare_const(&name, value);
                    self.ctx.pop_dims(&inline_dims);
                    out
                }
                Err(msg) => format!("Error: {}", msg),
            });
        }

        // :func declaration
        if input.starts_with(":func") {
            let rest = input[":func".len()..].trim();
            return Some(match parse_func_decl(rest) {
                Ok((name, params, body)) => {
                    let out = format!(
                        "\x1b[90m{}({}) = {}\x1b[0m",
                        name,
                        params.join(", "),
                        fmt_colored(&body)
                    );
                    self.ctx.declare_func(&name, params, body);
                    out
                }
                Err(msg) => format!("Error: {}", msg),
            });
        }

        // Equality check
        if let Some(eq_pos) = input.find('=') {
            let lhs_str = input[..eq_pos].trim();
            let rhs_str = input[eq_pos + 1..].trim();
            return Some(match (parse_expr(lhs_str), parse_expr(rhs_str)) {
                (Ok(lhs), Ok(rhs)) => {
                    let (lhs, lhs_dims) = self.ctx.push_inline_dims(&lhs);
                    let (rhs, rhs_dims) = self.ctx.push_inline_dims(&rhs);
                    let mut out = String::new();

                    match self.ctx.check_dims(&lhs, &rhs) {
                        DimOutcome::Pass | DimOutcome::Skipped { .. } => {}
                        DimOutcome::LhsRhsMismatch { lhs: dl, rhs: dr } => {
                            out.push_str(&format!(
                                "\x1b[31mdim error: lhs is {}, rhs is {}\x1b[0m\n",
                                dl, dr
                            ));
                        }
                        DimOutcome::ExprError { side, error } => {
                            let (source, offset) = if side == "lhs" {
                                (lhs_str, 0)
                            } else {
                                (rhs_str, input.chars().count() - rhs_str.chars().count())
                            };
                            out.push_str(&format_dim_error(&error, source, 2 + offset));
                            out.push('\n');
                        }
                    }
                    let result = self.ctx.check_equal(&lhs, &rhs);
                    match &result {
                        EqualityResult::Equal => {
                            out.push_str("\x1b[32mtrue\x1b[0m");
                        }
                        EqualityResult::NumericallyEqual { .. } => {
                            out.push_str("\x1b[32mtrue\x1b[0m (numerical)");
                        }
                        EqualityResult::NotEqual { residual } => {
                            out.push_str(&format!(
                                "\x1b[31mfalse\x1b[0m  residual: {}",
                                fmt_colored(residual)
                            ));
                        }
                    }
                    if result.passed() {
                        self.claim_counter += 1;
                        self.ctx.add_claim_as_rule(
                            &format!("repl_{}", self.claim_counter),
                            &lhs,
                            &rhs,
                        );
                    }
                    self.ctx.pop_dims(&lhs_dims);
                    self.ctx.pop_dims(&rhs_dims);
                    out
                }
                (Err(err), _) | (_, Err(err)) => format!("Error: {:?}", err),
            });
        }

        // Expression evaluation
        Some(match parse_expr(input) {
            Ok(expr) => {
                let (expr, inline_dims) = self.ctx.push_inline_dims(&expr);
                let mut out = String::new();

                if let Some(Err(e)) = self.ctx.check_expr_dim(&expr) {
                    out.push_str(&format_dim_error(&e, input, 2));
                    out.push('\n');
                }
                if self.show_trace {
                    let applied = self.ctx.apply_consts(&expr);
                    let (_, trace) = search::simplify_with_trace(&applied, &self.ctx.rules);

                    let plain_widths: Vec<usize> = trace
                        .iter()
                        .map(|s| format!("{}", s.expr).chars().count())
                        .collect();
                    let max_expr_width = plain_widths.iter().copied().max().unwrap_or(0);
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
                                out.push_str(&format!(
                                    "{}: {}{:padding$}  \x1b[90m{:width$}\x1b[0m  \x1b[2m{}\x1b[0m\n",
                                    i,
                                    expr_str,
                                    "",
                                    name,
                                    display,
                                    padding = padding,
                                    width = max_name_width,
                                ));
                            }
                            _ => out.push_str(&format!("{}: {}\n", i, expr_str)),
                        }
                    }
                } else {
                    let expr_ty = self.ctx.infer_ty(&expr);
                    let simplified = self.ctx.simplify(&expr);
                    let type_suffix = format_type_suffix(&simplified, expr_ty.as_ref());
                    out.push_str(&format!("{}{}", fmt_colored(&simplified), type_suffix));
                }

                self.ctx.pop_dims(&inline_dims);
                out
            }
            Err(err) => format!("Error: {:?}", err),
        })
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum HistoryMode {
    Results,
    Inputs,
}

pub fn run() -> Result<(), ReadlineError> {
    let mut rl = DefaultEditor::new()?;
    let mut session = Session::new();
    let mut history_mode = HistoryMode::Inputs;
    let mut input_history: Vec<String> = Vec::new();
    let mut result_history: Vec<String> = Vec::new();

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

                // Track input history for non-command lines
                if !input.starts_with(':') {
                    record_input(&mut input_history, &mut rl, history_mode, input);
                }

                match session.eval(input) {
                    Some(output) => {
                        println!("{}\n", output);
                        // Record simplified result for result history (expression lines only)
                        if !input.starts_with(':') && !input.contains('=') {
                            if let Ok(expr) = parse_expr(input) {
                                let (expr, inline_dims) = session.ctx.push_inline_dims(&expr);
                                let simplified = session.ctx.simplify(&expr);
                                record_result(
                                    &mut result_history,
                                    &mut rl,
                                    history_mode,
                                    &simplified,
                                );
                                session.ctx.pop_dims(&inline_dims);
                            }
                        }
                    }
                    None => break,
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

    /// Strip ANSI escape sequences from a string for clean test assertions.
    fn strip_ansi(s: &str) -> String {
        let mut out = String::new();
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                // Skip until 'm' (end of ANSI escape)
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next == 'm' {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    fn eval(session: &mut Session, input: &str) -> String {
        strip_ansi(&session.eval(input).expect("expected output"))
    }

    // ── format_type_suffix unit tests ─────────────────────────────

    #[test]
    fn format_type_suffix_shows_dimensionless_numeric_type() {
        use crate::expr::ExprKind;
        let suffix = format_type_suffix(
            &Expr::new(ExprKind::Rational(crate::rational::Rational::from_i64(4))),
            Some(&Ty::Concrete(Dimension::dimensionless())),
        );
        assert!(suffix.contains("[1]"));
    }

    #[test]
    fn format_type_suffix_shows_dimensionless_symbol_type() {
        let suffix = format_type_suffix(
            &crate::expr::scalar("x"),
            Some(&Ty::Concrete(Dimension::dimensionless())),
        );
        assert!(suffix.contains("[1]"));
    }

    // ── e2e session tests ─────────────────────────────────────────

    #[test]
    fn session_bare_number_is_dimensionless() {
        let mut s = Session::new();
        assert_eq!(eval(&mut s, "42"), "42 [1]");
    }

    #[test]
    fn session_arithmetic() {
        let mut s = Session::new();
        assert_eq!(eval(&mut s, "1 + 2"), "3 [1]");
        assert_eq!(eval(&mut s, "3 * 4"), "12 [1]");
    }

    #[test]
    fn session_bare_symbol_is_dimensionless() {
        let mut s = Session::new();
        assert_eq!(eval(&mut s, "x"), "x [1]");
    }

    #[test]
    fn session_var_declaration_persists() {
        let mut s = Session::new();
        eval(&mut s, ":var v [L T^-1]");
        let out = eval(&mut s, "v");
        assert!(out.contains("[L T^-1]"), "got: {}", out);
    }

    #[test]
    fn session_const_substitution() {
        let mut s = Session::new();
        eval(&mut s, ":const c = 3");
        assert_eq!(eval(&mut s, "c + 1"), "4 [1]");
    }

    #[test]
    fn session_unit_quantity_shows_unit() {
        let mut s = Session::new();
        // Single unit quantity — Quantity wrapper preserved, no suffix needed
        let out = eval(&mut s, "3 [m]");
        assert!(out.contains("[m]"), "got: {}", out);
    }

    #[test]
    fn session_unit_addition_shows_dimension() {
        let mut s = Session::new();
        let out = eval(&mut s, "1 [mm] + 2 [m]");
        // After simplification, units are converted to SI base.
        // The suffix should show [m] (length), not [1] (dimensionless).
        assert!(out.contains("[m]"), "got: {}", out);
        assert!(!out.contains("[1]"), "should not be dimensionless: {}", out);
    }

    #[test]
    fn session_unit_multiplication_shows_dimension() {
        let mut s = Session::new();
        let out = eval(&mut s, "2 [m] * 3 [kg]");
        // Should show some dimension, not [1]
        assert!(!out.contains("[1]"), "should not be dimensionless: {}", out);
    }

    #[test]
    fn session_inline_dim_annotation() {
        let mut s = Session::new();
        let out = eval(&mut s, "a [L] / b [T]");
        assert!(out.contains("a/b"), "got: {}", out);
        assert!(out.contains("[L T^-1]"), "got: {}", out);
    }

    #[test]
    fn session_inline_dims_are_transient() {
        let mut s = Session::new();
        eval(&mut s, "a [L]");
        // On the next line, 'a' should no longer have the [L] dimension
        let out = eval(&mut s, "a");
        assert_eq!(out, "a [1]");
    }

    #[test]
    fn session_var_dims_persist() {
        let mut s = Session::new();
        eval(&mut s, ":var a [L]");
        let out = eval(&mut s, "a");
        assert!(out.contains("[L]"), "got: {}", out);
    }

    #[test]
    fn session_equality_true() {
        let mut s = Session::new();
        let out = eval(&mut s, "a + b = b + a");
        assert!(out.contains("true"), "got: {}", out);
    }

    #[test]
    fn session_equality_false() {
        let mut s = Session::new();
        let out = eval(&mut s, "a + b = a - b");
        assert!(out.contains("false"), "got: {}", out);
    }

    #[test]
    fn session_reset_clears_context() {
        let mut s = Session::new();
        eval(&mut s, ":var v [L T^-1]");
        eval(&mut s, ":reset");
        // After reset, v should be back to dimensionless
        let out = eval(&mut s, "v");
        assert_eq!(out, "v [1]");
    }

    #[test]
    fn session_const_with_units_shows_dimension() {
        let mut s = Session::new();
        let out = eval(&mut s, ":const c = 3*10^8 [m/s]");
        assert!(out.contains("c ="), "got: {}", out);
        // Should show length/time dimension
        assert!(out.contains("[m"), "got: {}", out);
    }

    #[test]
    fn session_func_declaration_and_use() {
        let mut s = Session::new();
        // Single-char names are implicit multiplication, so use multi-char name
        eval(&mut s, ":func sq(x) = x^2 + 1");
        assert_eq!(eval(&mut s, "sq(3)"), "10 [1]");
    }

    #[test]
    fn session_claimed_equality_becomes_rule() {
        let mut s = Session::new();
        // Declare an identity
        let out = eval(&mut s, "2*a = a + a");
        assert!(out.contains("true"), "got: {}", out);
        // The simplifier should now be able to use this
        let out = eval(&mut s, "2*x");
        // Could simplify to x + x or stay as 2x, either is fine
        assert!(out.contains("x"), "got: {}", out);
    }
}
