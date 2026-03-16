use crate::expr::{FnKind, NamedConst};
use crate::random_search::{ChildIndex, IndexedRuleSet, SearchRun};
use crate::token::{path_to_position, tokenize, DeBruijn, Token};
use serde::{Deserialize, Serialize};
use std::io::Write;

/// One action in a simplification trace for ML training.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingAction {
    pub rule_direction: u16,
    pub position: usize,
}

/// One complete training example: an expression and its simplification trace.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
        FnKind::Custom(name) => return name.to_uppercase(),
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

    // Training data is defined over the explicit untyped token projection.
    // strip_types() here so that complexity and token-position measurements
    // operate on the same untyped shape that tokenize() serializes.
    let initial = run.initial.strip_types();
    let (input_tokens, _db) = tokenize(&initial);
    let input_token_strings: Vec<String> = input_tokens.iter().map(token_to_string).collect();
    let input_complexity = initial.complexity();

    // Convert each trace step to an action
    let mut actions = Vec::new();
    let mut current_expr = initial;

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
        current_expr = step.expr.strip_types();
    }

    Some(TrainingExample {
        input_tokens: input_token_strings,
        actions,
        output_complexity: run.result.strip_types().complexity(),
        input_complexity,
    })
}

/// Write a batch of TrainingExamples to a writer in JSONL format (one JSON object per line).
pub fn write_jsonl<W: Write>(examples: &[TrainingExample], writer: &mut W) -> std::io::Result<()> {
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

/// Error type for token string parsing failures.
#[derive(Debug)]
pub enum TokenParseError {
    UnknownToken(String),
}

impl std::fmt::Display for TokenParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenParseError::UnknownToken(s) => write!(f, "unknown token: {}", s),
        }
    }
}

/// Parse a token string back to a Token.
/// This is the inverse of `token_to_string`.
pub fn parse_token_string(s: &str) -> Result<Token, TokenParseError> {
    match s {
        "ADD" => Ok(Token::Add),
        "MUL" => Ok(Token::Mul),
        "POW" => Ok(Token::Pow),
        "NEG" => Ok(Token::Neg),
        "INV" => Ok(Token::Inv),
        "FRAC" => Ok(Token::Frac),
        "FRAC_PI" => Ok(Token::FracPi),
        "IDX_LO" => Ok(Token::IdxLo),
        "IDX_HI" => Ok(Token::IdxHi),
        // Named constants
        "E" => Ok(Token::Named(NamedConst::E)),
        "SQRT2" => Ok(Token::Named(NamedConst::Sqrt2)),
        "SQRT3" => Ok(Token::Named(NamedConst::Sqrt3)),
        "INV_SQRT2" => Ok(Token::Named(NamedConst::Frac1Sqrt2)),
        "SQRT3_2" => Ok(Token::Named(NamedConst::FracSqrt3By2)),
        // Multi-arg functions → FnN
        "MIN" => Ok(Token::FnN(FnKind::Min)),
        "MAX" => Ok(Token::FnN(FnKind::Max)),
        "CLAMP" => Ok(Token::FnN(FnKind::Clamp)),
        // Single-arg functions → Fn
        "SIN" => Ok(Token::Fn(FnKind::Sin)),
        "COS" => Ok(Token::Fn(FnKind::Cos)),
        "TAN" => Ok(Token::Fn(FnKind::Tan)),
        "ASIN" => Ok(Token::Fn(FnKind::Asin)),
        "ACOS" => Ok(Token::Fn(FnKind::Acos)),
        "ATAN" => Ok(Token::Fn(FnKind::Atan)),
        "SIGN" => Ok(Token::Fn(FnKind::Sign)),
        "SINH" => Ok(Token::Fn(FnKind::Sinh)),
        "COSH" => Ok(Token::Fn(FnKind::Cosh)),
        "TANH" => Ok(Token::Fn(FnKind::Tanh)),
        "FLOOR" => Ok(Token::Fn(FnKind::Floor)),
        "CEIL" => Ok(Token::Fn(FnKind::Ceil)),
        "ROUND" => Ok(Token::Fn(FnKind::Round)),
        "EXP" => Ok(Token::Fn(FnKind::Exp)),
        "LN" => Ok(Token::Fn(FnKind::Ln)),
        _ => {
            // Variable: V0, V1, ...
            if let Some(rest) = s.strip_prefix('V') {
                if let Ok(id) = rest.parse::<u16>() {
                    return Ok(Token::Var(id));
                }
            }
            // Integer: I_-3, I_0, I_5, ...
            if let Some(rest) = s.strip_prefix("I_") {
                if let Ok(n) = rest.parse::<i64>() {
                    return Ok(Token::Int(n));
                }
            }
            // Tensor index: IX0, IX1, ...
            if let Some(rest) = s.strip_prefix("IX") {
                if let Ok(id) = rest.parse::<u16>() {
                    return Ok(Token::Idx(id));
                }
            }
            Err(TokenParseError::UnknownToken(s.to_string()))
        }
    }
}

/// Build a synthetic DeBruijn mapping from token values.
/// Scans for max Var and Idx IDs, then creates names v0, v1, ... and i0, i1, ...
pub fn synthetic_debruijn(tokens: &[Token]) -> DeBruijn {
    let max_var = tokens
        .iter()
        .filter_map(|t| match t {
            Token::Var(id) => Some(*id),
            _ => None,
        })
        .max()
        .map(|m| m + 1)
        .unwrap_or(0);

    let max_idx = tokens
        .iter()
        .filter_map(|t| match t {
            Token::Idx(id) => Some(*id),
            _ => None,
        })
        .max()
        .map(|m| m + 1)
        .unwrap_or(0);

    DeBruijn::from_synthetic(max_var, max_idx)
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
    fn search_run_to_example_strips_quantity_boundary() {
        let unit = crate::unit::Unit {
            dimension: crate::dim::Dimension::single(crate::dim::BaseDim::L, 1),
            scale: 1.0,
            display: "m".to_string(),
        };
        let initial = quantity(rational(5, 1), unit);
        let result = rational(5, 1);
        let run = SearchRun {
            seed: 0,
            initial,
            result: result.clone(),
            trace: vec![RichTraceStep {
                expr: result,
                rule_index: 0,
                direction_id: RuleDirectionId(0),
                direction: Direction::Ltr,
                path: vec![],
                rule_name: "strip_quantity".to_string(),
            }],
            final_complexity: 2,
        };

        let example = search_run_to_example(&run).unwrap();
        assert_eq!(example.input_tokens, vec!["I_5"]);
        assert_eq!(example.input_complexity, 1);
        assert_eq!(example.output_complexity, 1);
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
    fn parse_token_string_roundtrip() {
        // All encoder tokens should roundtrip through token_to_string → parse_token_string
        let rules = IndexedRuleSet::new(RuleSet::full());
        let vocab = build_vocab_metadata(&rules);
        for tok_str in &vocab.encoder_tokens {
            let token = parse_token_string(tok_str)
                .unwrap_or_else(|_| panic!("failed to parse: {}", tok_str));
            assert_eq!(
                &token_to_string(&token),
                tok_str,
                "roundtrip failed for {}",
                tok_str
            );
        }
    }

    #[test]
    fn parse_token_string_unknown() {
        assert!(parse_token_string("UNKNOWN").is_err());
        assert!(parse_token_string("").is_err());
    }

    #[test]
    fn synthetic_debruijn_builds_correctly() {
        let tokens = vec![Token::Var(0), Token::Var(2), Token::Idx(1)];
        let db = synthetic_debruijn(&tokens);
        assert_eq!(db.var_name(0), Some("v0"));
        assert_eq!(db.var_name(1), Some("v1"));
        assert_eq!(db.var_name(2), Some("v2"));
        assert_eq!(db.var_name(3), None);
        assert_eq!(db.idx_name(0), Some("i0"));
        assert_eq!(db.idx_name(1), Some("i1"));
        assert_eq!(db.idx_name(2), None);
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
