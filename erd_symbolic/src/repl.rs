use crate::parser::parse_expr;
use crate::rule::RuleSet;
use crate::search;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

pub fn run() -> Result<(), ReadlineError> {
    let mut rl = DefaultEditor::new()?;

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

                match parse_expr(input) {
                    Ok(expr) => {
                        let simplified = search::simplify(&expr, &RuleSet::full());
                        println!("{}\n", simplified);
                        let _ = rl.add_history_entry(format!("{}", simplified));
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
