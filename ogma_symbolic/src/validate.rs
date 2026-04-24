use crate::expr::Expr;
use crate::random_search::{Direction, IndexedRuleSet, RuleDirectionId};
use crate::search::{eval_constants, TraceStep};
use crate::token::{position_to_path, replace_subexpr, subexpr_at, tokenize};

/// A single predicted action to validate.
#[derive(Debug, Clone)]
pub struct PredictedAction {
    pub rule_direction: u16,
    pub position: usize,
}

/// Per-step detail of what happened during validation.
#[derive(Debug, Clone)]
pub struct StepDetail {
    pub success: bool,
    pub failure_reason: Option<String>,
}

/// Result of validating an entire action sequence.
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub valid_steps: usize,
    pub total_steps: usize,
    pub final_expr: Expr,
    pub final_complexity: usize,
    pub input_complexity: usize,
    pub step_details: Vec<StepDetail>,
}

/// Validate a sequence of predicted actions against an expression.
///
/// For each action:
/// 1. Tokenize the current expression to get position→path mapping
/// 2. Convert the token position to an AST path
/// 3. Look up the rule and direction from the direction ID
/// 4. Get the subexpression at that path
/// 5. Apply the rule (LTR or RTL) at the subexpression root
/// 6. Replace the subexpression and run eval_constants
/// 7. If any step fails, stop — remaining actions are invalid
pub fn validate_action_sequence(
    initial_expr: &Expr,
    actions: &[PredictedAction],
    rules: &IndexedRuleSet,
) -> ValidationResult {
    let input_complexity = initial_expr.complexity();
    let mut current_expr = initial_expr.clone();
    let mut valid_steps = 0;
    let total_steps = actions.len();
    let mut step_details = Vec::new();

    for action in actions {
        // Look up the rule and direction
        let dir_id = RuleDirectionId(action.rule_direction);
        let (rule_index, direction) = match rules.lookup_direction(dir_id) {
            Some(rd) => rd,
            None => {
                step_details.push(StepDetail {
                    success: false,
                    failure_reason: Some(format!("invalid direction ID {}", action.rule_direction)),
                });
                break;
            }
        };

        // Tokenize current expression and get position→path mapping
        let (tokens, _db) = tokenize(&current_expr);
        let paths = position_to_path(&tokens);

        // Get the AST path for this position
        let path = match paths.get(action.position).and_then(|p| p.as_ref()) {
            Some(p) => p.clone(),
            None => {
                step_details.push(StepDetail {
                    success: false,
                    failure_reason: Some(format!(
                        "position {} has no AST path (token count {})",
                        action.position,
                        tokens.len()
                    )),
                });
                break;
            }
        };

        // Get the subexpression at this path
        let sub = match subexpr_at(&current_expr, &path) {
            Some(s) => s.clone(),
            None => {
                step_details.push(StepDetail {
                    success: false,
                    failure_reason: Some("path does not resolve to subexpression".to_string()),
                });
                break;
            }
        };

        // Apply the rule
        let rule = rules.rule(rule_index);
        let rewritten = match direction {
            Direction::Ltr => rule.apply_ltr(&sub),
            Direction::Rtl => rule.apply_rtl(&sub),
        };

        let rewritten = match rewritten {
            Some(r) => r,
            None => {
                step_details.push(StepDetail {
                    success: false,
                    failure_reason: Some(format!(
                        "rule '{}' ({:?}) did not match",
                        rule.name, direction
                    )),
                });
                break;
            }
        };

        // Replace the subexpression and eval_constants
        let new_expr = if path.is_empty() {
            eval_constants(&rewritten)
        } else {
            match replace_subexpr(&current_expr, &path, rewritten) {
                Some(e) => eval_constants(&e),
                None => {
                    step_details.push(StepDetail {
                        success: false,
                        failure_reason: Some("replace_subexpr failed".to_string()),
                    });
                    break;
                }
            }
        };

        current_expr = new_expr;
        valid_steps += 1;
        step_details.push(StepDetail {
            success: true,
            failure_reason: None,
        });
    }

    let final_complexity = current_expr.complexity();
    ValidationResult {
        valid_steps,
        total_steps,
        final_expr: current_expr,
        final_complexity,
        input_complexity,
        step_details,
    }
}

/// Validate a sequence of predicted actions and build a trace with intermediate expressions.
///
/// Returns both the `ValidationResult` and a `Vec<TraceStep>` suitable for REPL display.
/// The trace starts with the initial expression (no rule applied) and includes
/// one entry per successful step.
pub fn validate_with_trace(
    initial_expr: &Expr,
    actions: &[PredictedAction],
    rules: &IndexedRuleSet,
) -> (ValidationResult, Vec<TraceStep>) {
    let input_complexity = initial_expr.complexity();
    let mut current_expr = initial_expr.clone();
    let mut valid_steps = 0;
    let total_steps = actions.len();
    let mut step_details = Vec::new();
    let mut trace = vec![TraceStep {
        expr: current_expr.clone(),
        rule_name: None,
        rule_display: None,
    }];

    for action in actions {
        let dir_id = RuleDirectionId(action.rule_direction);
        let (rule_index, direction) = match rules.lookup_direction(dir_id) {
            Some(rd) => rd,
            None => {
                step_details.push(StepDetail {
                    success: false,
                    failure_reason: Some(format!("invalid direction ID {}", action.rule_direction)),
                });
                break;
            }
        };

        let (tokens, _db) = tokenize(&current_expr);
        let paths = position_to_path(&tokens);

        let path = match paths.get(action.position).and_then(|p| p.as_ref()) {
            Some(p) => p.clone(),
            None => {
                step_details.push(StepDetail {
                    success: false,
                    failure_reason: Some(format!(
                        "position {} has no AST path (token count {})",
                        action.position,
                        tokens.len()
                    )),
                });
                break;
            }
        };

        let sub = match subexpr_at(&current_expr, &path) {
            Some(s) => s.clone(),
            None => {
                step_details.push(StepDetail {
                    success: false,
                    failure_reason: Some("path does not resolve to subexpression".to_string()),
                });
                break;
            }
        };

        let rule = rules.rule(rule_index);
        let rewritten = match direction {
            Direction::Ltr => rule.apply_ltr(&sub),
            Direction::Rtl => rule.apply_rtl(&sub),
        };

        let rewritten = match rewritten {
            Some(r) => r,
            None => {
                step_details.push(StepDetail {
                    success: false,
                    failure_reason: Some(format!(
                        "rule '{}' ({:?}) did not match",
                        rule.name, direction
                    )),
                });
                break;
            }
        };

        let new_expr = if path.is_empty() {
            eval_constants(&rewritten)
        } else {
            match replace_subexpr(&current_expr, &path, rewritten) {
                Some(e) => eval_constants(&e),
                None => {
                    step_details.push(StepDetail {
                        success: false,
                        failure_reason: Some("replace_subexpr failed".to_string()),
                    });
                    break;
                }
            }
        };

        current_expr = new_expr;
        valid_steps += 1;
        step_details.push(StepDetail {
            success: true,
            failure_reason: None,
        });

        let dir_str = match direction {
            Direction::Ltr => "→",
            Direction::Rtl => "←",
        };
        trace.push(TraceStep {
            expr: current_expr.clone(),
            rule_name: Some(rule.name.clone()),
            rule_display: Some(format!("{} {}", rule.name, dir_str)),
        });
    }

    let final_complexity = current_expr.complexity();
    let result = ValidationResult {
        valid_steps,
        total_steps,
        final_expr: current_expr,
        final_complexity,
        input_complexity,
        step_details,
    };
    (result, trace)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::*;
    use crate::random_search::IndexedRuleSet;
    use crate::rule::RuleSet;

    fn full_rules() -> IndexedRuleSet {
        IndexedRuleSet::new(RuleSet::full())
    }

    /// Find the direction ID for a named rule in a given direction.
    fn find_direction_id(rules: &IndexedRuleSet, name: &str, direction: Direction) -> Option<u16> {
        for i in 0..rules.len() {
            if rules.rule(i).name == name {
                return match direction {
                    Direction::Ltr => Some(rules.ltr_id(i).0),
                    Direction::Rtl => rules.rtl_id(i).map(|id| id.0),
                };
            }
        }
        None
    }

    #[test]
    fn validate_empty_sequence() {
        let rules = full_rules();
        let expr = add(scalar("x"), rational(0, 1));
        let result = validate_action_sequence(&expr, &[], &rules);
        assert_eq!(result.valid_steps, 0);
        assert_eq!(result.total_steps, 0);
        assert_eq!(result.input_complexity, 3);
        assert_eq!(result.final_complexity, 3); // unchanged
    }

    #[test]
    fn validate_single_valid_action() {
        let rules = full_rules();
        let expr = add(scalar("x"), rational(0, 1)); // x + 0

        let dir_id = find_direction_id(&rules, "add_zero_right", Direction::Ltr)
            .expect("add_zero_right should exist");

        let actions = vec![PredictedAction {
            rule_direction: dir_id,
            position: 0, // root
        }];

        let result = validate_action_sequence(&expr, &actions, &rules);
        assert_eq!(result.valid_steps, 1);
        assert_eq!(result.total_steps, 1);
        assert_eq!(result.final_complexity, 1); // just x
        assert!(result.step_details[0].success);
    }

    #[test]
    fn validate_invalid_direction_id() {
        let rules = full_rules();
        let expr = scalar("x");
        let actions = vec![PredictedAction {
            rule_direction: 9999,
            position: 0,
        }];

        let result = validate_action_sequence(&expr, &actions, &rules);
        assert_eq!(result.valid_steps, 0);
        assert_eq!(result.total_steps, 1);
        assert!(!result.step_details[0].success);
    }

    #[test]
    fn validate_invalid_position() {
        let rules = full_rules();
        let expr = scalar("x"); // only 1 token

        let dir_id = find_direction_id(&rules, "add_zero_right", Direction::Ltr).unwrap();
        let actions = vec![PredictedAction {
            rule_direction: dir_id,
            position: 99, // way out of range
        }];

        let result = validate_action_sequence(&expr, &actions, &rules);
        assert_eq!(result.valid_steps, 0);
        assert_eq!(result.total_steps, 1);
    }

    #[test]
    fn validate_rule_no_match() {
        let rules = full_rules();
        let expr = add(scalar("x"), scalar("y")); // x + y, not x + 0

        let dir_id = find_direction_id(&rules, "add_zero_right", Direction::Ltr).unwrap();
        let actions = vec![PredictedAction {
            rule_direction: dir_id,
            position: 0, // root
        }];

        let result = validate_action_sequence(&expr, &actions, &rules);
        assert_eq!(result.valid_steps, 0);
        assert_eq!(result.total_steps, 1);
    }

    #[test]
    fn validate_partial_sequence() {
        let rules = full_rules();
        // (x + 0) + y → should simplify the left child
        let expr = add(add(scalar("x"), rational(0, 1)), scalar("y"));

        let dir_id = find_direction_id(&rules, "add_zero_right", Direction::Ltr).unwrap();

        let actions = vec![
            // First action: apply add_zero_right at position 1 (left child = x + 0)
            PredictedAction {
                rule_direction: dir_id,
                position: 1,
            },
            // Second action: invalid (add_zero_right on x + y doesn't match)
            PredictedAction {
                rule_direction: dir_id,
                position: 0,
            },
        ];

        let result = validate_action_sequence(&expr, &actions, &rules);
        assert_eq!(result.valid_steps, 1);
        assert_eq!(result.total_steps, 2);
        // After first step: x + y (complexity 3)
        assert_eq!(result.final_complexity, 3);
    }

    #[test]
    fn validate_with_trace_empty_sequence() {
        let rules = full_rules();
        let expr = add(scalar("x"), rational(0, 1));
        let (result, trace) = validate_with_trace(&expr, &[], &rules);
        assert_eq!(result.valid_steps, 0);
        assert_eq!(result.total_steps, 0);
        // Trace starts with the initial expression
        assert_eq!(trace.len(), 1);
        assert!(trace[0].rule_name.is_none());
    }

    #[test]
    fn validate_with_trace_single_valid_action() {
        let rules = full_rules();
        let expr = add(scalar("x"), rational(0, 1)); // x + 0

        let dir_id = find_direction_id(&rules, "add_zero_right", Direction::Ltr)
            .expect("add_zero_right should exist");

        let actions = vec![PredictedAction {
            rule_direction: dir_id,
            position: 0,
        }];

        let (result, trace) = validate_with_trace(&expr, &actions, &rules);
        assert_eq!(result.valid_steps, 1);
        // Trace: initial + 1 successful step
        assert_eq!(trace.len(), 2);
        assert!(trace[0].rule_name.is_none());
        assert_eq!(trace[1].rule_name.as_deref(), Some("add_zero_right"));
        assert!(trace[1].rule_display.as_ref().unwrap().contains("→"));
    }

    #[test]
    fn validate_with_trace_invalid_direction_id() {
        let rules = full_rules();
        let expr = scalar("x");
        let actions = vec![PredictedAction {
            rule_direction: 9999,
            position: 0,
        }];

        let (result, trace) = validate_with_trace(&expr, &actions, &rules);
        assert_eq!(result.valid_steps, 0);
        // Only the initial expression in trace
        assert_eq!(trace.len(), 1);
        assert!(!result.step_details[0].success);
        assert!(result.step_details[0]
            .failure_reason
            .as_ref()
            .unwrap()
            .contains("invalid direction ID"));
    }

    #[test]
    fn validate_with_trace_invalid_position() {
        let rules = full_rules();
        let expr = scalar("x");

        let dir_id = find_direction_id(&rules, "add_zero_right", Direction::Ltr).unwrap();
        let actions = vec![PredictedAction {
            rule_direction: dir_id,
            position: 99,
        }];

        let (result, trace) = validate_with_trace(&expr, &actions, &rules);
        assert_eq!(result.valid_steps, 0);
        assert_eq!(trace.len(), 1);
        assert!(result.step_details[0]
            .failure_reason
            .as_ref()
            .unwrap()
            .contains("no AST path"));
    }

    #[test]
    fn validate_with_trace_rule_no_match() {
        let rules = full_rules();
        let expr = add(scalar("x"), scalar("y")); // x + y, not x + 0

        let dir_id = find_direction_id(&rules, "add_zero_right", Direction::Ltr).unwrap();
        let actions = vec![PredictedAction {
            rule_direction: dir_id,
            position: 0,
        }];

        let (result, trace) = validate_with_trace(&expr, &actions, &rules);
        assert_eq!(result.valid_steps, 0);
        assert_eq!(trace.len(), 1);
        assert!(result.step_details[0]
            .failure_reason
            .as_ref()
            .unwrap()
            .contains("did not match"));
    }

    #[test]
    fn validate_with_trace_multi_step() {
        let rules = full_rules();
        let expr = add(
            add(scalar("x"), rational(0, 1)),
            add(scalar("y"), rational(0, 1)),
        );

        let dir_id = find_direction_id(&rules, "add_zero_right", Direction::Ltr).unwrap();

        let actions = vec![
            PredictedAction {
                rule_direction: dir_id,
                position: 1,
            },
            PredictedAction {
                rule_direction: dir_id,
                position: 2,
            },
        ];

        let (result, trace) = validate_with_trace(&expr, &actions, &rules);
        assert_eq!(result.valid_steps, 2);
        assert_eq!(trace.len(), 3); // initial + 2 steps
        assert!(trace[0].rule_name.is_none());
        assert!(trace[1].rule_name.is_some());
        assert!(trace[2].rule_name.is_some());
    }

    #[test]
    fn validate_position_relative_to_current() {
        let rules = full_rules();
        // (x + 0) + (y + 0) → x + (y + 0) → x + y
        let expr = add(
            add(scalar("x"), rational(0, 1)),
            add(scalar("y"), rational(0, 1)),
        );

        let dir_id = find_direction_id(&rules, "add_zero_right", Direction::Ltr).unwrap();

        // Tokens of initial: [ADD, ADD, V0, I_0, ADD, V1, I_0]
        //                      0    1    2   3    4    5   6
        // Position 1 = left child (x + 0)

        let actions = vec![
            PredictedAction {
                rule_direction: dir_id,
                position: 1, // left child (x + 0) in initial expression
            },
            // After first step, expr is x + (y + 0)
            // Tokens: [ADD, V0, ADD, V1, I_0]
            //          0    1   2    3   4
            // Position 2 = right child (y + 0)
            PredictedAction {
                rule_direction: dir_id,
                position: 2,
            },
        ];

        let result = validate_action_sequence(&expr, &actions, &rules);
        assert_eq!(result.valid_steps, 2);
        assert_eq!(result.total_steps, 2);
        assert_eq!(result.final_complexity, 3); // x + y
    }
}
