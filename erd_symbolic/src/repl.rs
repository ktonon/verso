use crate::parser::parse_expr;
use crate::rule::RuleSet;
use crate::search;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

pub fn run() -> Result<(), ReadlineError> {
    let mut rl = DefaultEditor::new()?;
    let mut show_steps = false;

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
                if input == ":steps" {
                    show_steps = !show_steps;
                    println!("steps: {}\n", if show_steps { "on" } else { "off" });
                    continue;
                }

                match parse_expr(input) {
                    Ok(expr) => {
                        if show_steps {
                            let (simplified, trace) =
                                search::simplify_with_trace(&expr, &RuleSet::full());
                            for (i, step) in trace.iter().enumerate() {
                                println!("{}: {}", i, step);
                            }
                            println!("final: {}\n", simplified);
                            let _ = rl.add_history_entry(format!("{}", simplified));
                        } else {
                            let simplified = search::simplify(&expr, &RuleSet::full());
                            println!("{}\n", simplified);
                            let _ = rl.add_history_entry(format!("{}", simplified));
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
