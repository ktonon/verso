use crate::parser::parse_expr;
use crate::rule::RuleSet;
use crate::search;
use std::io::{self, Write};

pub fn run() -> io::Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        write!(stdout, "> ")?;
        stdout.flush()?;

        let mut line = String::new();
        if stdin.read_line(&mut line)? == 0 {
            break;
        }

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
                writeln!(stdout, "{}\n", simplified)?;
            }
            Err(err) => {
                writeln!(stdout, "Error: {:?}", err)?;
            }
        }
    }

    Ok(())
}
