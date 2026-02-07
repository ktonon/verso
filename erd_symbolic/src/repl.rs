use crate::fmt::fmt_colored;
use crate::parser::parse_expr;
use crate::rule::RuleSet;
use crate::search;
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
                        if show_trace {
                            let (simplified, trace) =
                                search::simplify_with_trace(&expr, &RuleSet::full());
                            for (i, step) in trace.iter().enumerate() {
                                println!("{}: {}", i, fmt_colored(step));
                            }
                            println!("final: {}\n", fmt_colored(&simplified));
                            record_result(&mut result_history, &mut rl, history_mode, &simplified);
                        } else {
                            let simplified = search::simplify(&expr, &RuleSet::full());
                            println!("{}\n", fmt_colored(&simplified));
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

fn reload_history(rl: &mut DefaultEditor, entries: &[String]) {
    let _ = rl.clear_history();
    for entry in entries {
        let _ = rl.add_history_entry(entry);
    }
}
