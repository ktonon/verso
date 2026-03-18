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
        let input = crate::unicode::replace_all(input.trim());
        let input = input.trim();
        if input.is_empty() || input == "!q" || input == "!quit" || input == "!exit" {
            return None;
        }
        if input == "?" || input == "?help" {
            return Some(help_all());
        }
        if let Some(topic) = input.strip_prefix('?') {
            return Some(help_topic(topic.trim()));
        }
        if input == "!trace" {
            self.show_trace = !self.show_trace;
            return Some(format!(
                "trace: {}",
                if self.show_trace { "on" } else { "off" }
            ));
        }
        if input == "!reset" {
            self.ctx = Context::new();
            self.claim_counter = 0;
            return Some("context reset".to_string());
        }

        // !var declaration
        if input.starts_with("!var") {
            let rest = input["!var".len()..].trim();
            return Some(match parse_var_decl(rest) {
                Ok((name, dim)) => {
                    self.ctx.declare_var(&name, Some(dim.clone()));
                    format!("\x1b[90m{}: {}\x1b[0m", name, dim)
                }
                Err(msg) => format!("Error: {}", msg),
            });
        }

        // !const declaration
        if input.starts_with("!const") {
            let rest = input["!const".len()..].trim();
            return Some(match parse_const_decl(rest) {
                Ok((name, value)) => {
                    let (stripped, inline_dims) = match self.ctx.push_inline_dims(&value) {
                        Ok(r) => r,
                        Err(e) => return Some(format!("Error: {}", e)),
                    };
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

        // !func declaration
        if input.starts_with("!func") {
            let rest = input["!func".len()..].trim();
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
                    let (lhs, lhs_dims) = match self.ctx.push_inline_dims(&lhs) {
                        Ok(r) => r,
                        Err(e) => return Some(format!("Error: {}", e)),
                    };
                    let (rhs, rhs_dims) = match self.ctx.push_inline_dims(&rhs) {
                        Ok(r) => r,
                        Err(e) => {
                            self.ctx.pop_dims(&lhs_dims);
                            return Some(format!("Error: {}", e));
                        }
                    };
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
                // Check for division by zero before simplification
                if contains_div_by_zero(&expr) {
                    return Some("Error: division by zero is undefined".to_string());
                }

                let (expr, inline_dims) = match self.ctx.push_inline_dims(&expr) {
                    Ok(r) => r,
                    Err(e) => return Some(format!("Error: {}", e)),
                };
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

/// Check if an expression contains division by zero (Inv of a zero constant).
fn contains_div_by_zero(expr: &Expr) -> bool {
    use crate::expr::ExprKind;
    expr.any(&|e| {
        matches!(
            &e.kind,
            ExprKind::Inv(inner) if matches!(&inner.kind, ExprKind::Rational(r) if r.is_zero())
        )
    })
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
                if input == "!q" || input == "!quit" || input == "!exit" {
                    break;
                }
                if input == "!history" || input == "!hist" {
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
                if !input.starts_with('!') {
                    record_input(&mut input_history, &mut rl, history_mode, input);
                }

                match session.eval(input) {
                    Some(output) => {
                        println!("{}\n", output);
                        // Record simplified result for result history (expression lines only)
                        if !input.starts_with('!') && !input.contains('=') {
                            if let Ok(expr) = parse_expr(input) {
                                if let Ok((expr, inline_dims)) = session.ctx.push_inline_dims(&expr)
                                {
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

struct HelpEntry {
    command: &'static str,
    summary: &'static str,
    detail: &'static str,
}

const HELP_ENTRIES: &[HelpEntry] = &[
    HelpEntry {
        command: "var",
        summary: "Declare a typed variable",
        detail: "\
!var <name> [<dimensions>]

Declares a variable with the given dimensional type.
The dimension persists for the rest of the session.

Examples:
  !var v [L T^-1]
  !var F [M L T^-2]
  !var θ [1]           dimensionless",
    },
    HelpEntry {
        command: "const",
        summary: "Declare a named constant",
        detail: "\
!const <name> = <expr>

Binds a name to an expression. The name is substituted
in subsequent expressions.

Examples:
  !const c = 3*10^8
  !const g = 9.81 [m/s^2]",
    },
    HelpEntry {
        command: "func",
        summary: "Declare a function",
        detail: "\
!func <name>(<params>) = <body>

Defines a function that can be called in expressions.

Examples:
  !func sq(x) = x^2
  !func ke(m, v) = m*v^2/2",
    },
    HelpEntry {
        command: "trace",
        summary: "Toggle step-by-step simplification trace",
        detail: "\
!trace

Shows each rewrite step applied during simplification,
including the rule name and pattern. Toggle on/off.",
    },
    HelpEntry {
        command: "reset",
        summary: "Clear all declarations and rules",
        detail: "\
!reset

Clears all variable, constant, and function declarations,
and any rules derived from verified equalities.",
    },
    HelpEntry {
        command: "history",
        summary: "Toggle between input and result history",
        detail: "\
!history  (alias: !hist)

Switches the up-arrow history between your inputs and
the simplified results. Default is input history.",
    },
    HelpEntry {
        command: "q",
        summary: "Quit the REPL",
        detail: "\
!q  (aliases: !quit, !exit)

Exits the REPL session.",
    },
];

fn help_all() -> String {
    let mut out = String::from("Commands:\n");
    for entry in HELP_ENTRIES {
        out.push_str(&format!("  !{:<10} {}\n", entry.command, entry.summary));
    }
    out.push_str("\nType ?<command> for details, e.g. ?var");
    out
}

fn help_topic(topic: &str) -> String {
    let topic = topic.strip_prefix('!').unwrap_or(topic);
    for entry in HELP_ENTRIES {
        if entry.command == topic {
            return entry.detail.to_string();
        }
    }
    format!("unknown command '{}' — type ? for a list", topic)
}

fn parse_var_decl(rest: &str) -> Result<(String, Dimension), String> {
    let bracket_pos = rest
        .find('[')
        .ok_or("!var requires name [dims], e.g. !var v [L T^-1]")?;
    let name = rest[..bracket_pos].trim().to_string();
    if name.is_empty() {
        return Err("!var requires a variable name".into());
    }
    let dim_str = rest[bracket_pos..].trim();
    let dimension = Dimension::parse(dim_str).map_err(|e| format!("{}", e))?;
    Ok((name, dimension))
}

fn parse_const_decl(rest: &str) -> Result<(String, Expr), String> {
    let eq_pos = rest
        .find('=')
        .ok_or("!const requires name = expr, e.g. !const c = 3*10^8")?;
    let name = rest[..eq_pos].trim().to_string();
    if name.is_empty() {
        return Err("!const requires a name".into());
    }
    let value_str = rest[eq_pos + 1..].trim();
    let value = parse_expr(value_str).map_err(|e| format!("{:?}", e))?;
    Ok((name, value))
}

fn parse_func_decl(rest: &str) -> Result<(String, Vec<String>, Expr), String> {
    let lparen = rest.find('(').ok_or("!func requires name(params) = expr")?;
    let name = rest[..lparen].trim().to_string();
    if name.is_empty() {
        return Err("!func requires a name".into());
    }
    let rparen = rest.find(')').ok_or("!func missing closing parenthesis")?;
    let params: Vec<String> = rest[lparen + 1..rparen]
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if params.is_empty() {
        return Err("!func requires at least one parameter".into());
    }
    let after = rest[rparen + 1..].trim();
    let body_str = after
        .strip_prefix('=')
        .ok_or("!func requires = after parameters")?
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

    /// Parse a REPL session transcript into (input, expected_output) pairs.
    ///
    /// Format:
    /// ```text
    /// > input line
    /// expected output
    ///
    /// > next input
    /// expected output
    /// ```
    ///
    /// - Lines starting with `> ` begin a new input.
    /// - Non-empty lines following an input are the expected output.
    /// - A `> ` line with no following output runs the command without asserting.
    fn parse_transcript(transcript: &str) -> Vec<(&str, String)> {
        let mut pairs = Vec::new();
        let mut current_input: Option<&str> = None;
        let mut output_lines: Vec<&str> = Vec::new();

        for line in transcript.trim().lines() {
            if let Some(input) = line.strip_prefix("> ") {
                if let Some(prev_input) = current_input.take() {
                    pairs.push((prev_input, output_lines.join("\n").trim().to_string()));
                    output_lines.clear();
                }
                current_input = Some(input);
            } else if current_input.is_some() && !line.trim().is_empty() {
                output_lines.push(line);
            }
        }

        if let Some(input) = current_input {
            pairs.push((input, output_lines.join("\n").trim().to_string()));
        }

        pairs
    }

    /// Run a REPL session transcript, asserting each output matches exactly.
    /// Inputs with no expected output are executed without assertion.
    macro_rules! session {
        ($transcript:expr) => {{
            let mut s = Session::new();
            for (input, expected) in parse_transcript($transcript) {
                if expected.is_empty() {
                    s.eval(input).expect("expected output");
                } else {
                    let actual = eval(&mut s, input);
                    assert_eq!(actual, expected, "\n  input: > {}", input);
                }
            }
        }};
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

    // ── e2e session tests: regression guards for fixed bugs ────────

    #[test]
    fn bug_zero_div_zero_should_be_undefined() {
        let mut s = Session::new();
        let out = eval(&mut s, "0/0");
        assert!(
            out.contains("undefined") || out.contains("Error"),
            "0/0 should be undefined, got: {}",
            out
        );
    }

    #[test]
    fn bug_sqrt_numeric_not_evaluated() {
        session!(
            r#"
> sqrt(4)
2 [1]
"#
        );
    }

    #[test]
    fn bug_neg_squared_not_collected() {
        session!(
            r#"
> (-x)^2
x^2 [1]
"#
        );
    }

    #[test]
    fn bug_abs_numeric_not_evaluated() {
        session!(
            r#"
> abs(-3)
3 [1]
"#
        );
    }

    #[test]
    fn bug_floor_numeric_not_evaluated() {
        session!(
            r#"
> floor(3/2)
1 [1]
"#
        );
    }

    #[test]
    fn bug_ceil_numeric_not_evaluated() {
        session!(
            r#"
> ceil(3/2)
2 [1]
"#
        );
    }

    #[test]
    fn bug_unit_quantity_multiplication() {
        session!(
            r#"
> 1 [kg] * 10 [m/s^2]
10 [N]
"#
        );
    }

    // ── e2e session tests ─────────────────────────────────────────

    #[test]
    fn session_bare_values() {
        session!(
            r#"
> 42
42 [1]

> x
x [1]
"#
        );
    }

    #[test]
    fn session_arithmetic() {
        session!(
            r#"
> 1 + 2
3 [1]

> 3 * 4
12 [1]
"#
        );
    }

    #[test]
    fn session_var_declaration_persists() {
        session!(
            r#"
> !var v [L T^-1]
v: [L T^-1]

> v
v [L T^-1]
"#
        );
    }

    #[test]
    fn session_const_substitution() {
        session!(
            r#"
> !const c = 3
c = 3 [1]

> c + 1
4 [1]
"#
        );
    }

    #[test]
    fn session_const_with_units() {
        session!(
            r#"
> !const g = 3*10^8 [m/s]
g = 300000000 [m/s]
"#
        );
    }

    #[test]
    fn session_unit_quantity() {
        session!(
            r#"
> 3 [m]
3 [m]

> 1 [mm] + 2 [m]
2001/1000 [m]
"#
        );
    }

    #[test]
    fn session_inline_dim_annotation() {
        session!(
            r#"
> a [L] / b [T]
a/b [L T^-1]
"#
        );
    }

    #[test]
    fn session_inline_dims_are_transient() {
        session!(
            r#"
> a [L]
a [L]

> a
a [1]
"#
        );
    }

    #[test]
    fn session_var_dims_persist() {
        session!(
            r#"
> !var a [L]
a: [L]

> a
a [L]

> a [L]
a [L]
"#
        );
    }

    #[test]
    fn session_var_dims_prevents_inline_override() {
        session!(
            r#"
> !var a [L]
a: [L]

> a [T]
Error: 'a' is declared [L], cannot override with inline [T]
"#
        );
    }

    #[test]
    fn session_equality() {
        session!(
            r#"
> a + b = b + a
true

> a + b = a - b
false  residual: 2b
"#
        );
    }

    #[test]
    fn session_reset_clears_context() {
        session!(
            r#"
> !var v [L T^-1]
v: [L T^-1]

> !reset
context reset

> v
v [1]
"#
        );
    }

    #[test]
    fn session_func_declaration_and_use() {
        session!(
            r#"
> !func sq(x) = x^2 + 1
sq(x) = x^2 + 1

> sq(3)
10 [1]
"#
        );
    }

    #[test]
    fn session_claimed_equality_becomes_rule() {
        session!(
            r#"
> 2*a = a + a
true

> 2*x
2x [1]
"#
        );
    }

    #[test]
    fn session_inline_dim_cannot_override_declared() {
        session!(
            r#"
> !var a [T]
a: [T]

> a [L]
Error: 'a' is declared [T], cannot override with inline [L]

> a [T]
a [T]
"#
        );
    }

    // ── parse_transcript unit tests ───────────────────────────────

    #[test]
    fn parse_transcript_basic() {
        let pairs = parse_transcript(
            r#"
> 1 + 2
3

> x
x [1]
"#,
        );
        assert_eq!(
            pairs,
            vec![("1 + 2", "3".to_string()), ("x", "x [1]".to_string())]
        );
    }

    // ── unicode completion tests ──────────────────────────────────

    #[test]
    fn session_unicode_replacement_in_expression() {
        session!(
            r#"
> :mu: + :nu:
μ + ν [1]
"#
        );
    }

    #[test]
    fn session_unicode_direct_input() {
        session!(
            r#"
> μ + ν
μ + ν [1]
"#
        );
    }

    #[test]
    fn session_unicode_in_var_declaration() {
        session!(
            r#"
> !var :mu: [M]
μ: [M]

> μ
μ [M]
"#
        );
    }

    #[test]
    fn session_unicode_in_const_declaration() {
        session!(
            r#"
> !const :alpha: = 3
α = 3 [1]

> :alpha:
3 [1]
"#
        );
    }

    #[test]
    fn session_unicode_partial_replacement() {
        // Only known names get replaced; unknown patterns pass through
        assert_eq!(
            crate::unicode::replace_all(":mu: + :unknown:"),
            "μ + :unknown:"
        );
    }

    // ── help tests ─────────────────────────────────────────────

    #[test]
    fn help_lists_all_commands() {
        let mut s = Session::new();
        let out = eval(&mut s, "?");
        assert!(out.contains("!var"), "should list !var");
        assert!(out.contains("!const"), "should list !const");
        assert!(out.contains("!func"), "should list !func");
        assert!(out.contains("!trace"), "should list !trace");
        assert!(out.contains("!reset"), "should list !reset");
        assert!(out.contains("!history"), "should list !history");
        assert!(out.contains("!q"), "should list !q");
    }

    #[test]
    fn help_specific_command() {
        let mut s = Session::new();
        let out = eval(&mut s, "?var");
        assert!(out.contains("!var"), "should mention !var");
        assert!(out.contains("[L T^-1]"), "should show example");
    }

    #[test]
    fn help_unknown_command() {
        let mut s = Session::new();
        let out = eval(&mut s, "?foo");
        assert!(out.contains("unknown"), "should say unknown");
    }

    #[test]
    fn parse_transcript_no_output_skips_assertion() {
        let pairs = parse_transcript(
            r#"
> !var v [L]
> v
v [L]
"#,
        );
        assert_eq!(
            pairs,
            vec![("!var v [L]", String::new()), ("v", "v [L]".to_string()),]
        );
    }
}
