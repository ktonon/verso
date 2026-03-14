use crate::dim::Dimension;
use crate::expr::Expr;
use crate::fmt::fmt_colored;
use crate::parser::parse_expr;
use crate::rule::RuleSet;
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
    let mut show_trace = false;
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

                record_input(&mut input_history, &mut rl, history_mode, input);

                match parse_expr(input) {
                    Ok(expr) => {
                        let input_dim = expr.first_unit().map(|u| u.dimension.clone());
                        if show_trace {
                            let (simplified, trace) =
                                search::simplify_with_trace(&expr, &RuleSet::full());

                            // Compute alignment widths using plain (non-colored) text
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
                            record_result(&mut result_history, &mut rl, history_mode, &simplified);
                        } else {
                            let simplified = search::simplify(&expr, &RuleSet::full());
                            let unit_suffix = format_unit_suffix(&simplified, input_dim.as_ref());
                            println!("{}{}\n", fmt_colored(&simplified), unit_suffix);
                            record_result(&mut result_history, &mut rl, history_mode, &simplified);
                        }
                    }
                    Err(err) => {
                        println!("Error: {:?}\n", err);
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
    simplified: &crate::expr::Expr,
) {
    let rendered = format!("{}", simplified);
    result_history.push(rendered.clone());
    if history_mode == HistoryMode::Results {
        let _ = rl.add_history_entry(rendered);
    }
}

fn format_unit_suffix(simplified: &Expr, input_dim: Option<&Dimension>) -> String {
    let dim = match input_dim {
        Some(d) => d,
        None => return String::new(),
    };
    // If the simplified expr still has units, the formatter shows them already
    if simplified.first_unit().is_some() {
        return String::new();
    }
    format!(" \x1b[36m[{}]\x1b[0m", base_si_display(dim))
}

fn reload_history(rl: &mut DefaultEditor, entries: &[String]) {
    let _ = rl.clear_history();
    for entry in entries {
        let _ = rl.add_history_entry(entry);
    }
}
