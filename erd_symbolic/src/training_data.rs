use crate::expr::{FnKind, NamedConst};
use crate::random_search::{ChildIndex, IndexedRuleSet, SearchRun};
use crate::token::{path_to_position, tokenize, Token};
use serde::Serialize;
use std::io::Write;

/// One action in a simplification trace for ML training.
#[derive(Debug, Clone, Serialize)]
pub struct TrainingAction {
    pub rule_direction: u16,
    pub position: usize,
}

/// One complete training example: an expression and its simplification trace.
#[derive(Debug, Clone, Serialize)]
pub struct TrainingExample {
    pub input_tokens: Vec<String>,
    pub actions: Vec<TrainingAction>,
    pub output_complexity: usize,
    pub input_complexity: usize,
}

/// Convert a random_search ChildIndex path to a token.rs usize path.
///
/// Mapping: Left→0, Right→1, Inner→0, Arg(n)→n.
/// This matches how token.rs numbers children in assign_paths.
pub fn convert_path(path: &[ChildIndex]) -> Vec<usize> {
    path.iter()
        .map(|ci| match ci {
            ChildIndex::Left => 0,
            ChildIndex::Right => 1,
            ChildIndex::Inner => 0,
            ChildIndex::Arg(n) => *n,
        })
        .collect()
}

/// Convert a Token to its ML-friendly string representation.
pub fn token_to_string(token: &Token) -> String {
    match token {
        Token::Add => "ADD".to_string(),
        Token::Mul => "MUL".to_string(),
        Token::Pow => "POW".to_string(),
        Token::Neg => "NEG".to_string(),
        Token::Inv => "INV".to_string(),
        Token::Fn(kind) | Token::FnN(kind) => fn_kind_string(kind),
        Token::Var(id) => format!("V{}", id),
        Token::Int(n) => format!("I_{}", n),
        Token::Frac => "FRAC".to_string(),
        Token::FracPi => "FRAC_PI".to_string(),
        Token::IdxLo => "IDX_LO".to_string(),
        Token::IdxHi => "IDX_HI".to_string(),
        Token::Idx(id) => format!("IX{}", id),
        Token::Named(nc) => named_const_string(nc),
    }
}

pub fn fn_kind_string(kind: &FnKind) -> String {
    match kind {
        FnKind::Sin => "SIN",
        FnKind::Cos => "COS",
        FnKind::Tan => "TAN",
        FnKind::Asin => "ASIN",
        FnKind::Acos => "ACOS",
        FnKind::Atan => "ATAN",
        FnKind::Sign => "SIGN",
        FnKind::Sinh => "SINH",
        FnKind::Cosh => "COSH",
        FnKind::Tanh => "TANH",
        FnKind::Floor => "FLOOR",
        FnKind::Ceil => "CEIL",
        FnKind::Round => "ROUND",
        FnKind::Min => "MIN",
        FnKind::Max => "MAX",
        FnKind::Clamp => "CLAMP",
        FnKind::Exp => "EXP",
        FnKind::Ln => "LN",
    }
    .to_string()
}

pub fn named_const_string(nc: &NamedConst) -> String {
    match nc {
        NamedConst::E => "E",
        NamedConst::Sqrt2 => "SQRT2",
        NamedConst::Sqrt3 => "SQRT3",
        NamedConst::Frac1Sqrt2 => "INV_SQRT2",
        NamedConst::FracSqrt3By2 => "SQRT3_2",
    }
    .to_string()
}

/// Convert a SearchRun into a TrainingExample.
///
/// Returns None if the trace is empty (no simplification found)
/// or if any position lookup fails.
///
/// Positions are per-step: each action's position is relative to the
/// expression at the time the rule was applied (initial for step 0,
/// trace[i-1].expr for step i>0).
pub fn search_run_to_example(run: &SearchRun) -> Option<TrainingExample> {
    if run.trace.is_empty() {
        return None;
    }

    // Tokenize the initial expression
    let (input_tokens, _db) = tokenize(&run.initial);
    let input_token_strings: Vec<String> = input_tokens.iter().map(token_to_string).collect();
    let input_complexity = run.initial.complexity();

    // Convert each trace step to an action
    let mut actions = Vec::new();
    let mut current_expr = run.initial.clone();

    for step in &run.trace {
        // Tokenize the current expression (before this step was applied)
        let (current_tokens, _) = tokenize(&current_expr);

        // Convert the ChildIndex path to a usize path
        let usize_path = convert_path(&step.path);

        // Find the token position for this path
        let position = path_to_position(&current_tokens, &usize_path)?;

        actions.push(TrainingAction {
            rule_direction: step.direction_id.0,
            position,
        });

        // Advance to the result of this step
        current_expr = step.expr.clone();
    }

    Some(TrainingExample {
        input_tokens: input_token_strings,
        actions,
        output_complexity: run.final_complexity,
        input_complexity,
    })
}

/// Write a batch of TrainingExamples to a writer in JSONL format (one JSON object per line).
pub fn write_jsonl<W: Write>(
    examples: &[TrainingExample],
    writer: &mut W,
) -> std::io::Result<()> {
    for example in examples {
        serde_json::to_writer(&mut *writer, example)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        writer.write_all(b"\n")?;
    }
    Ok(())
}

/// One entry in the rule direction vocabulary.
#[derive(Debug, Clone, Serialize)]
pub struct RuleDirectionEntry {
    pub id: u16,
    pub name: String,
    pub direction: String,
}

/// Complete vocabulary metadata for the Python training code.
#[derive(Debug, Clone, Serialize)]
pub struct VocabMetadata {
    pub encoder_tokens: Vec<String>,
    pub rule_directions: Vec<RuleDirectionEntry>,
    pub total_directions: u16,
}

/// All FnKind variants in a fixed order for vocabulary enumeration.
const ALL_FN_KINDS: &[FnKind] = &[
    FnKind::Sin,
    FnKind::Cos,
    FnKind::Tan,
    FnKind::Asin,
    FnKind::Acos,
    FnKind::Atan,
    FnKind::Sign,
    FnKind::Sinh,
    FnKind::Cosh,
    FnKind::Tanh,
    FnKind::Floor,
    FnKind::Ceil,
    FnKind::Round,
    FnKind::Min,
    FnKind::Max,
    FnKind::Clamp,
    FnKind::Exp,
    FnKind::Ln,
];

/// All NamedConst variants in a fixed order for vocabulary enumeration.
const ALL_NAMED_CONSTS: &[NamedConst] = &[
    NamedConst::E,
    NamedConst::Sqrt2,
    NamedConst::Sqrt3,
    NamedConst::Frac1Sqrt2,
    NamedConst::FracSqrt3By2,
];

/// Build vocabulary metadata from the indexed rule set.
///
/// Encoder tokens cover: operators, all FnKind variants, all NamedConst variants,
/// structural tokens, variables V0..V7, integers I_{-10}..I_{20}, indices IX0..IX3.
pub fn build_vocab_metadata(indexed: &IndexedRuleSet) -> VocabMetadata {
    let mut encoder_tokens = Vec::new();

    // Operators
    for tok in &[Token::Add, Token::Mul, Token::Pow, Token::Neg, Token::Inv] {
        encoder_tokens.push(token_to_string(tok));
    }

    // All function kinds
    for kind in ALL_FN_KINDS {
        encoder_tokens.push(fn_kind_string(kind));
    }

    // All named constants
    for nc in ALL_NAMED_CONSTS {
        encoder_tokens.push(named_const_string(nc));
    }

    // Structural tokens
    for tok in &[Token::Frac, Token::FracPi, Token::IdxLo, Token::IdxHi] {
        encoder_tokens.push(token_to_string(tok));
    }

    // Variables V0..V7
    for i in 0..8u16 {
        encoder_tokens.push(format!("V{}", i));
    }

    // Integers I_{-10}..I_{20}
    for i in -10..=20i64 {
        encoder_tokens.push(format!("I_{}", i));
    }

    // Tensor indices IX0..IX3
    for i in 0..4u16 {
        encoder_tokens.push(format!("IX{}", i));
    }

    // Rule directions
    let mut rule_directions = Vec::new();
    for i in 0..indexed.len() {
        let rule = indexed.rule(i);
        rule_directions.push(RuleDirectionEntry {
            id: indexed.ltr_id(i).0,
            name: rule.name.clone(),
            direction: "ltr".to_string(),
        });
    }
    for i in 0..indexed.len() {
        if let Some(rtl_id) = indexed.rtl_id(i) {
            let rule = indexed.rule(i);
            rule_directions.push(RuleDirectionEntry {
                id: rtl_id.0,
                name: rule.name.clone(),
                direction: "rtl".to_string(),
            });
        }
    }
    // Sort by id for deterministic output
    rule_directions.sort_by_key(|e| e.id);

    VocabMetadata {
        encoder_tokens,
        total_directions: indexed.total_directions,
        rule_directions,
    }
}

/// Write vocabulary metadata as pretty JSON.
pub fn write_vocab_json<W: Write>(metadata: &VocabMetadata, writer: &mut W) -> std::io::Result<()> {
    serde_json::to_writer_pretty(writer, metadata)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::*;
    use crate::random_search::{
        Direction, IndexedRuleSet, RandomizedBeamSearch, RichTraceStep, RuleDirectionId,
    };
    use crate::rule::RuleSet;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    #[test]
    fn convert_path_variants() {
        assert_eq!(convert_path(&[]), Vec::<usize>::new());
        assert_eq!(convert_path(&[ChildIndex::Left]), vec![0]);
        assert_eq!(convert_path(&[ChildIndex::Right]), vec![1]);
        assert_eq!(convert_path(&[ChildIndex::Inner]), vec![0]);
        assert_eq!(convert_path(&[ChildIndex::Arg(2)]), vec![2]);
        assert_eq!(
            convert_path(&[ChildIndex::Left, ChildIndex::Right]),
            vec![0, 1]
        );
    }

    #[test]
    fn token_to_string_operators() {
        assert_eq!(token_to_string(&Token::Add), "ADD");
        assert_eq!(token_to_string(&Token::Mul), "MUL");
        assert_eq!(token_to_string(&Token::Pow), "POW");
        assert_eq!(token_to_string(&Token::Neg), "NEG");
        assert_eq!(token_to_string(&Token::Inv), "INV");
    }

    #[test]
    fn token_to_string_functions() {
        assert_eq!(token_to_string(&Token::Fn(FnKind::Sin)), "SIN");
        assert_eq!(token_to_string(&Token::Fn(FnKind::Cos)), "COS");
        assert_eq!(token_to_string(&Token::Fn(FnKind::Exp)), "EXP");
        assert_eq!(token_to_string(&Token::Fn(FnKind::Ln)), "LN");
        assert_eq!(token_to_string(&Token::FnN(FnKind::Min)), "MIN");
        assert_eq!(token_to_string(&Token::FnN(FnKind::Max)), "MAX");
    }

    #[test]
    fn token_to_string_named() {
        assert_eq!(token_to_string(&Token::Named(NamedConst::E)), "E");
        assert_eq!(token_to_string(&Token::Named(NamedConst::Sqrt2)), "SQRT2");
        assert_eq!(
            token_to_string(&Token::Named(NamedConst::Frac1Sqrt2)),
            "INV_SQRT2"
        );
    }

    #[test]
    fn token_to_string_var_int() {
        assert_eq!(token_to_string(&Token::Var(0)), "V0");
        assert_eq!(token_to_string(&Token::Var(3)), "V3");
        assert_eq!(token_to_string(&Token::Int(0)), "I_0");
        assert_eq!(token_to_string(&Token::Int(5)), "I_5");
        assert_eq!(token_to_string(&Token::Int(-3)), "I_-3");
    }

    #[test]
    fn search_run_to_example_empty_trace() {
        let run = SearchRun {
            seed: 0,
            initial: scalar("x"),
            result: scalar("x"),
            trace: vec![],
            final_complexity: 1,
        };
        assert!(search_run_to_example(&run).is_none());
    }

    #[test]
    fn search_run_to_example_basic() {
        // Construct a SearchRun for x + 0 → x
        let initial = add(scalar("x"), rational(0, 1));
        let run = SearchRun {
            seed: 0,
            initial: initial.clone(),
            result: scalar("x"),
            trace: vec![RichTraceStep {
                expr: scalar("x"),
                rule_index: 0,
                direction_id: RuleDirectionId(0),
                direction: Direction::Ltr,
                path: vec![], // root
                rule_name: "add_zero_right".to_string(),
            }],
            final_complexity: 1,
        };

        let example = search_run_to_example(&run).unwrap();
        // x + 0 tokenizes to: [ADD, V0, I_0]
        assert_eq!(example.input_tokens, vec!["ADD", "V0", "I_0"]);
        assert_eq!(example.actions.len(), 1);
        assert_eq!(example.actions[0].position, 0); // root
        assert_eq!(example.actions[0].rule_direction, 0);
        assert_eq!(example.input_complexity, 3); // Add + Var + Rational
        assert_eq!(example.output_complexity, 1); // just Var
    }

    #[test]
    fn example_serialization() {
        let example = TrainingExample {
            input_tokens: vec!["ADD".to_string(), "V0".to_string(), "I_0".to_string()],
            actions: vec![TrainingAction {
                rule_direction: 5,
                position: 0,
            }],
            output_complexity: 1,
            input_complexity: 3,
        };

        let json = serde_json::to_string(&example).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(
            parsed["input_tokens"],
            serde_json::json!(["ADD", "V0", "I_0"])
        );
        assert_eq!(parsed["actions"][0]["rule_direction"], 5);
        assert_eq!(parsed["actions"][0]["position"], 0);
        assert_eq!(parsed["output_complexity"], 1);
        assert_eq!(parsed["input_complexity"], 3);
    }

    #[test]
    fn write_jsonl_format() {
        let examples = vec![
            TrainingExample {
                input_tokens: vec!["V0".to_string()],
                actions: vec![],
                output_complexity: 1,
                input_complexity: 1,
            },
            TrainingExample {
                input_tokens: vec!["ADD".to_string(), "V0".to_string(), "I_1".to_string()],
                actions: vec![TrainingAction {
                    rule_direction: 0,
                    position: 0,
                }],
                output_complexity: 1,
                input_complexity: 3,
            },
        ];

        let mut buf = Vec::new();
        write_jsonl(&examples, &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();
        let lines: Vec<&str> = output.trim().split('\n').collect();
        assert_eq!(lines.len(), 2);

        // Each line should be valid JSON
        for line in &lines {
            let parsed: Result<serde_json::Value, _> = serde_json::from_str(line);
            assert!(parsed.is_ok(), "invalid JSON: {}", line);
        }
    }

    #[test]
    fn build_vocab_metadata_sanity() {
        let rules = IndexedRuleSet::new(RuleSet::full());
        let vocab = build_vocab_metadata(&rules);

        // Encoder tokens: 5 ops + 18 fns + 5 named + 4 structural + 8 vars + 31 ints + 4 indices = 75
        assert_eq!(vocab.encoder_tokens.len(), 75);
        assert_eq!(vocab.encoder_tokens[0], "ADD");
        assert!(vocab.encoder_tokens.contains(&"SIN".to_string()));
        assert!(vocab.encoder_tokens.contains(&"E".to_string()));
        assert!(vocab.encoder_tokens.contains(&"FRAC_PI".to_string()));
        assert!(vocab.encoder_tokens.contains(&"V0".to_string()));
        assert!(vocab.encoder_tokens.contains(&"V7".to_string()));
        assert!(vocab.encoder_tokens.contains(&"I_-10".to_string()));
        assert!(vocab.encoder_tokens.contains(&"I_20".to_string()));
        assert!(vocab.encoder_tokens.contains(&"IX0".to_string()));
        assert!(vocab.encoder_tokens.contains(&"IX3".to_string()));

        // Rule directions should match total_directions
        assert_eq!(vocab.rule_directions.len(), vocab.total_directions as usize);
        assert_eq!(vocab.total_directions, rules.total_directions);

        // First direction should be LTR with id 0
        assert_eq!(vocab.rule_directions[0].id, 0);
        assert_eq!(vocab.rule_directions[0].direction, "ltr");

        // Serializes to valid JSON
        let mut buf = Vec::new();
        write_vocab_json(&vocab, &mut buf).unwrap();
        let json: serde_json::Value =
            serde_json::from_slice(&buf).expect("vocab.json should be valid JSON");
        assert!(json["encoder_tokens"].is_array());
        assert!(json["rule_directions"].is_array());
        assert!(json["total_directions"].is_number());
    }

    #[test]
    fn end_to_end_gen_and_convert() {
        let rules = IndexedRuleSet::new(RuleSet::full());
        let search = RandomizedBeamSearch {
            epsilon: 0.0,
            shuffle_rules: false,
            ..Default::default()
        };

        // x + 0 should simplify, giving a non-empty trace
        let expr = add(scalar("x"), rational(0, 1));
        let mut rng = StdRng::seed_from_u64(42);
        let run = search.search_once(&expr, &rules, &mut rng);

        if !run.trace.is_empty() {
            let example = search_run_to_example(&run);
            assert!(example.is_some(), "conversion should succeed");
            let example = example.unwrap();

            // Verify it serializes to valid JSON
            let json = serde_json::to_string(&example).unwrap();
            let parsed: Result<serde_json::Value, _> = serde_json::from_str(&json);
            assert!(parsed.is_ok());
        }
    }
}
