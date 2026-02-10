use burn::data::dataloader::batcher::Batcher;
use burn::data::dataset::Dataset;
use burn::prelude::*;

use erd_symbolic::random_search::IndexedRuleSet;
use erd_symbolic::training_data::TrainingExample;
use erd_symbolic::token::tokenize;
use erd_symbolic::training_data::token_to_string;
use erd_symbolic::validate::{validate_action_sequence, PredictedAction};

use erd_symbolic::eval_constants;

use crate::evaluate::tokens_to_expr;
use crate::vocab::EncoderVocab;

/// A single per-step training item for the policy model.
#[derive(Debug, Clone)]
pub struct PolicyItem {
    pub enc_ids: Vec<i64>,
    pub rule_target: i64,
    pub position_target: i64,
}

/// Expand a TrainingExample (multi-step) into per-step PolicyItems.
///
/// For each action in the example:
/// 1. Tokenize the current expression and encode with EncoderVocab
/// 2. Record (enc_ids, rule_direction, position) as a PolicyItem
/// 3. Apply the action to get the next expression
/// 4. Stop if any action fails to apply
fn expand_to_policy_items(
    example: &TrainingExample,
    enc_vocab: &EncoderVocab,
    rules: &IndexedRuleSet,
) -> Vec<PolicyItem> {
    let mut items = Vec::new();

    // Reconstruct the initial expression from tokens, then normalize.
    // eval_constants is also applied at inference time (policy_inference_loop)
    // and after each action (validate_action_sequence), so training data must match.
    let mut current_expr = match tokens_to_expr(&example.input_tokens) {
        Ok(expr) => eval_constants(&expr),
        Err(_) => return items,
    };

    for action in &example.actions {
        // Tokenize current expression and encode
        let (tokens, _db) = tokenize(&current_expr);
        let enc_ids: Vec<i64> = tokens
            .iter()
            .map(|t| enc_vocab.encode(&token_to_string(t)) as i64)
            .collect();

        // Skip if position target is out of bounds (would land in pad region)
        if action.position as usize >= enc_ids.len() {
            break;
        }

        items.push(PolicyItem {
            enc_ids,
            rule_target: action.rule_direction as i64,
            position_target: action.position as i64,
        });

        // Apply this single action to advance to the next expression
        let predicted = PredictedAction {
            rule_direction: action.rule_direction,
            position: action.position,
        };
        let result = validate_action_sequence(&current_expr, &[predicted], rules);
        if result.valid_steps == 0 {
            break; // Action failed; remaining actions are invalid
        }
        current_expr = result.final_expr;
    }

    items
}

/// Dataset of per-step policy items expanded from multi-step TrainingExamples.
pub struct PolicyDataset {
    items: Vec<PolicyItem>,
}

impl PolicyDataset {
    pub fn from_examples(
        examples: &[TrainingExample],
        enc_vocab: &EncoderVocab,
        rules: &IndexedRuleSet,
    ) -> Self {
        let items: Vec<PolicyItem> = examples
            .iter()
            .flat_map(|ex| expand_to_policy_items(ex, enc_vocab, rules))
            .collect();
        Self { items }
    }
}

impl Dataset<PolicyItem> for PolicyDataset {
    fn get(&self, index: usize) -> Option<PolicyItem> {
        self.items.get(index).cloned()
    }

    fn len(&self) -> usize {
        self.items.len()
    }
}

/// Batched tensors for the policy model.
#[derive(Debug, Clone)]
pub struct PolicyBatch<B: Backend> {
    pub enc_ids: Tensor<B, 2, Int>,
    pub enc_pad_mask: Tensor<B, 2, Bool>,
    pub rule_targets: Tensor<B, 1, Int>,
    pub position_targets: Tensor<B, 1, Int>,
}

/// Batcher that pads encoder sequences and stacks targets.
#[derive(Clone)]
pub struct PolicyBatcher;

impl<B: Backend> Batcher<B, PolicyItem, PolicyBatch<B>> for PolicyBatcher {
    fn batch(&self, items: Vec<PolicyItem>, device: &B::Device) -> PolicyBatch<B> {
        let batch_size = items.len();
        let max_len = items.iter().map(|i| i.enc_ids.len()).max().unwrap_or(0);

        // Build pad mask from actual lengths (not token ID == 0),
        // so unknown tokens encoded as PAD(0) aren't falsely masked.
        let mask_flat: Vec<bool> = items
            .iter()
            .flat_map(|i| {
                let real_len = i.enc_ids.len();
                (0..max_len).map(move |j| j >= real_len)
            })
            .collect();

        // Pad encoder sequences
        let flat: Vec<i64> = items
            .iter()
            .flat_map(|i| {
                let mut padded = i.enc_ids.clone();
                padded.resize(max_len, 0);
                padded
            })
            .collect();
        let enc_ids = Tensor::<B, 2, Int>::from_data(
            TensorData::new(flat, [batch_size, max_len]),
            device,
        );
        let enc_pad_mask = Tensor::<B, 2, Bool>::from_data(
            TensorData::new(mask_flat, [batch_size, max_len]),
            device,
        );

        // Stack scalar targets
        let rule_data: Vec<i64> = items.iter().map(|i| i.rule_target).collect();
        let pos_data: Vec<i64> = items.iter().map(|i| i.position_target).collect();

        let rule_targets = Tensor::<B, 1, Int>::from_data(
            TensorData::new(rule_data, [batch_size]),
            device,
        );
        let position_targets = Tensor::<B, 1, Int>::from_data(
            TensorData::new(pos_data, [batch_size]),
            device,
        );

        PolicyBatch {
            enc_ids,
            enc_pad_mask,
            rule_targets,
            position_targets,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use burn::backend::NdArray;
    use erd_symbolic::random_search::IndexedRuleSet;
    use erd_symbolic::training_data::TrainingAction;
    use erd_symbolic::RuleSet;

    type TestBackend = NdArray;

    fn setup() -> (IndexedRuleSet, EncoderVocab) {
        let indexed = IndexedRuleSet::new(RuleSet::full());
        let enc_vocab = EncoderVocab::new(&indexed);
        (indexed, enc_vocab)
    }

    #[test]
    fn test_expand_single_step() {
        let (rules, enc_vocab) = setup();

        // x + 0 → x (add_zero_right rule, position 0 at root)
        // Find the rule direction for add_zero_right LTR
        let ex = TrainingExample {
            input_tokens: vec!["ADD".to_string(), "V0".to_string(), "I_0".to_string()],
            actions: vec![TrainingAction {
                rule_direction: 0, // Will be looked up dynamically
                position: 0,
            }],
            output_complexity: 1,
            input_complexity: 3,
        };

        // We need to find the actual rule direction for add_zero_right
        // Just test that we get 1 item (the action may fail if rule_direction 0 doesn't match,
        // but at least the first tokenization should succeed)
        let items = expand_to_policy_items(&ex, &enc_vocab, &rules);
        // We should get at least the first item (tokenization of the initial expression)
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].enc_ids.len(), 3); // ADD, V0, I_0
        assert!(items[0].enc_ids.iter().all(|&id| id > 0), "no PAD tokens in real input");
    }

    #[test]
    fn test_expand_multi_step() {
        let (rules, enc_vocab) = setup();

        // Create a 2-step example with rule_direction 0 and 1
        // The actual actions may fail to validate, but we should get items for each
        // action where the previous action succeeded
        let ex = TrainingExample {
            input_tokens: vec![
                "ADD".to_string(),
                "ADD".to_string(),
                "V0".to_string(),
                "I_0".to_string(),
                "V1".to_string(),
            ],
            actions: vec![
                TrainingAction {
                    rule_direction: 0,
                    position: 1,
                },
                TrainingAction {
                    rule_direction: 0,
                    position: 0,
                },
            ],
            output_complexity: 1,
            input_complexity: 5,
        };

        let items = expand_to_policy_items(&ex, &enc_vocab, &rules);
        // First item always succeeds (tokenize initial), second depends on action validity
        assert!(!items.is_empty(), "should have at least 1 item");
        // Verify first item has 5 tokens
        assert_eq!(items[0].enc_ids.len(), 5);
    }

    #[test]
    fn test_dataset_from_examples() {
        let (rules, enc_vocab) = setup();

        let examples = vec![
            TrainingExample {
                input_tokens: vec!["ADD".to_string(), "V0".to_string(), "I_0".to_string()],
                actions: vec![TrainingAction {
                    rule_direction: 0,
                    position: 0,
                }],
                output_complexity: 1,
                input_complexity: 3,
            },
            TrainingExample {
                input_tokens: vec!["MUL".to_string(), "V0".to_string(), "I_1".to_string()],
                actions: vec![TrainingAction {
                    rule_direction: 0,
                    position: 0,
                }],
                output_complexity: 1,
                input_complexity: 3,
            },
        ];

        let dataset = PolicyDataset::from_examples(&examples, &enc_vocab, &rules);
        assert_eq!(dataset.len(), 2); // One item per example (single-step each)
    }

    #[test]
    fn test_batcher_shapes() {
        let (rules, enc_vocab) = setup();

        let examples = vec![
            TrainingExample {
                input_tokens: vec!["ADD".to_string(), "V0".to_string(), "I_0".to_string()],
                actions: vec![TrainingAction {
                    rule_direction: 0,
                    position: 0,
                }],
                output_complexity: 1,
                input_complexity: 3,
            },
            TrainingExample {
                input_tokens: vec![
                    "MUL".to_string(),
                    "NEG".to_string(),
                    "V0".to_string(),
                    "I_3".to_string(),
                ],
                actions: vec![TrainingAction {
                    rule_direction: 0,
                    position: 0,
                }],
                output_complexity: 1,
                input_complexity: 4,
            },
        ];

        let dataset = PolicyDataset::from_examples(&examples, &enc_vocab, &rules);
        let items: Vec<_> = (0..dataset.len()).map(|i| dataset.get(i).unwrap()).collect();

        let batcher = PolicyBatcher;
        let device = &burn::backend::ndarray::NdArrayDevice::Cpu;
        let batch: PolicyBatch<TestBackend> = batcher.batch(items, device);

        assert_eq!(batch.enc_ids.dims()[0], 2); // batch size
        assert_eq!(batch.enc_ids.dims()[1], 4); // max(3, 4)
        assert_eq!(batch.rule_targets.dims(), [2]);
        assert_eq!(batch.position_targets.dims(), [2]);
    }

    #[test]
    fn test_items_with_real_data() {
        let data_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("data_training");
        if !data_dir.exists() {
            eprintln!("Skipping test_items_with_real_data: data_training/ not found");
            return;
        }

        let (rules, enc_vocab) = setup();
        let (train, _val) = crate::dataset::load_data(data_dir.to_str().unwrap(), 0.1, 42);

        // Expand first 100 examples
        let examples = &train[..100.min(train.len())];
        let dataset = PolicyDataset::from_examples(examples, &enc_vocab, &rules);

        println!(
            "Expanded {} examples into {} policy items ({:.1}x)",
            examples.len(),
            dataset.len(),
            dataset.len() as f64 / examples.len() as f64
        );
        assert!(dataset.len() >= examples.len(), "should have at least as many items as examples");
    }
}
