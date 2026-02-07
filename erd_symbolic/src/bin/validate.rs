use erd_symbolic::random_search::IndexedRuleSet;
use erd_symbolic::token::{detokenize, tokenize};
use erd_symbolic::training_data::{parse_token_string, synthetic_debruijn, token_to_string};
use erd_symbolic::validate::{validate_action_sequence, PredictedAction};
use erd_symbolic::RuleSet;
use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, BufWriter, Write};

#[derive(Deserialize)]
struct InputEntry {
    id: usize,
    input_tokens: Vec<String>,
    actions: Vec<ActionEntry>,
}

#[derive(Deserialize)]
struct ActionEntry {
    rule_direction: u16,
    position: usize,
}

#[derive(Serialize)]
struct OutputEntry {
    id: usize,
    valid_steps: usize,
    total_steps: usize,
    input_complexity: usize,
    final_complexity: usize,
    output_tokens: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn write_error(writer: &mut impl Write, id: usize, total_steps: usize, error: String) {
    let entry = OutputEntry {
        id,
        valid_steps: 0,
        total_steps,
        input_complexity: 0,
        final_complexity: 0,
        output_tokens: vec![],
        error: Some(error),
    };
    serde_json::to_writer(&mut *writer, &entry).unwrap();
    writer.write_all(b"\n").unwrap();
}

fn main() {
    let rules = IndexedRuleSet::new(RuleSet::full());
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut writer = BufWriter::new(stdout.lock());

    for line in stdin.lock().lines() {
        let line = line.expect("failed to read line");
        if line.trim().is_empty() {
            continue;
        }

        let entry: InputEntry = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(e) => {
                write_error(&mut writer, 0, 0, format!("parse error: {}", e));
                continue;
            }
        };

        // Parse token strings to Token values
        let tokens: Result<Vec<_>, _> = entry
            .input_tokens
            .iter()
            .map(|s| parse_token_string(s))
            .collect();
        let tokens = match tokens {
            Ok(t) => t,
            Err(e) => {
                write_error(
                    &mut writer,
                    entry.id,
                    entry.actions.len(),
                    format!("token parse: {}", e),
                );
                continue;
            }
        };

        // Build synthetic DeBruijn and reconstruct expression
        let db = synthetic_debruijn(&tokens);
        let expr = match detokenize(&tokens, &db) {
            Ok(e) => e,
            Err(e) => {
                write_error(
                    &mut writer,
                    entry.id,
                    entry.actions.len(),
                    format!("detokenize: {:?}", e),
                );
                continue;
            }
        };

        // Build predicted actions
        let actions: Vec<PredictedAction> = entry
            .actions
            .iter()
            .map(|a| PredictedAction {
                rule_direction: a.rule_direction,
                position: a.position,
            })
            .collect();

        // Validate
        let result = validate_action_sequence(&expr, &actions, &rules);

        // Re-tokenize the final expression for output
        let (final_tokens, _) = tokenize(&result.final_expr);
        let output_tokens: Vec<String> = final_tokens.iter().map(token_to_string).collect();

        let out = OutputEntry {
            id: entry.id,
            valid_steps: result.valid_steps,
            total_steps: result.total_steps,
            input_complexity: result.input_complexity,
            final_complexity: result.final_complexity,
            output_tokens,
            error: None,
        };
        serde_json::to_writer(&mut writer, &out).unwrap();
        writer.write_all(b"\n").unwrap();
    }
}
