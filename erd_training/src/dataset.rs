use burn::data::dataloader::batcher::Batcher;
use burn::data::dataset::Dataset;
use burn::prelude::*;

use erd_symbolic::training_data::TrainingExample;

use crate::vocab::{DecoderVocab, EncoderVocab};

/// A single processed training item (token IDs, not yet batched/padded).
#[derive(Debug, Clone)]
pub struct SimplificationItem {
    pub enc_ids: Vec<i64>,
    pub dec_input: Vec<i64>,
    pub dec_target: Vec<i64>,
}

/// Load all JSONL files from a directory, shuffle, and split into train/val.
pub fn load_data(
    data_dir: &str,
    val_fraction: f64,
    seed: u64,
) -> (Vec<TrainingExample>, Vec<TrainingExample>) {
    let mut files: Vec<_> = std::fs::read_dir(data_dir)
        .unwrap_or_else(|_| panic!("Cannot read directory: {}", data_dir))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "jsonl"))
        .collect();
    files.sort();

    assert!(!files.is_empty(), "No .jsonl files found in {}", data_dir);

    let mut examples: Vec<TrainingExample> = Vec::new();
    for path in &files {
        let content = std::fs::read_to_string(path)
            .unwrap_or_else(|_| panic!("Cannot read file: {}", path.display()));
        for line in content.lines() {
            if !line.trim().is_empty() {
                let ex: TrainingExample = serde_json::from_str(line)
                    .unwrap_or_else(|e| panic!("Failed to parse JSONL line: {}", e));
                examples.push(ex);
            }
        }
    }

    // Deterministic shuffle using the same algorithm as Python's random.Random(seed).shuffle
    // We use a simple Fisher-Yates shuffle with our own seed.
    use rand::rngs::StdRng;
    use rand::seq::SliceRandom;
    use rand::SeedableRng;
    let mut rng = StdRng::seed_from_u64(seed);
    examples.shuffle(&mut rng);

    let split = (examples.len() as f64 * (1.0 - val_fraction)) as usize;
    let val = examples.split_off(split);
    (examples, val)
}

/// Dataset that converts TrainingExamples to SimplificationItems using vocabularies.
pub struct SimplificationDataset {
    items: Vec<SimplificationItem>,
}

impl SimplificationDataset {
    pub fn new(
        examples: &[TrainingExample],
        enc_vocab: &EncoderVocab,
        dec_vocab: &DecoderVocab,
        max_actions: usize,
    ) -> Self {
        let items = examples
            .iter()
            .map(|ex| {
                // Encode input tokens
                let enc_ids: Vec<i64> = ex
                    .input_tokens
                    .iter()
                    .map(|t| enc_vocab.encode(t) as i64)
                    .collect();

                // Build decoder sequence: interleaved RULE/POS tokens + STOP
                let actions = &ex.actions[..ex.actions.len().min(max_actions)];
                let mut dec_tokens: Vec<i64> = Vec::new();
                for action in actions {
                    dec_tokens.push(dec_vocab.encode_rule(action.rule_direction) as i64);
                    dec_tokens.push(dec_vocab.encode_pos(action.position) as i64);
                }
                dec_tokens.push(DecoderVocab::STOP as i64);

                // Teacher forcing: input is BOS + all but last, target is the full sequence
                let mut dec_input = vec![DecoderVocab::BOS as i64];
                dec_input.extend_from_slice(&dec_tokens[..dec_tokens.len() - 1]);
                let dec_target = dec_tokens;

                SimplificationItem {
                    enc_ids,
                    dec_input,
                    dec_target,
                }
            })
            .collect();

        Self { items }
    }
}

impl Dataset<SimplificationItem> for SimplificationDataset {
    fn get(&self, index: usize) -> Option<SimplificationItem> {
        self.items.get(index).cloned()
    }

    fn len(&self) -> usize {
        self.items.len()
    }
}

/// A batched set of tensors ready for the model.
#[derive(Debug, Clone)]
pub struct SimplificationBatch<B: Backend> {
    pub enc_ids: Tensor<B, 2, Int>,
    pub enc_pad_mask: Tensor<B, 2, Bool>,
    pub dec_input: Tensor<B, 2, Int>,
    pub dec_target: Tensor<B, 2, Int>,
    pub dec_pad_mask: Tensor<B, 2, Bool>,
}

/// Pads variable-length sequences and produces batch tensors.
#[derive(Clone)]
pub struct SimplificationBatcher;

/// Pad a batch of variable-length i64 sequences to the same length.
fn pad_sequences(seqs: &[Vec<i64>], pad_value: i64) -> (Vec<Vec<i64>>, usize) {
    let max_len = seqs.iter().map(|s| s.len()).max().unwrap_or(0);
    let padded: Vec<Vec<i64>> = seqs
        .iter()
        .map(|s| {
            let mut p = s.clone();
            p.resize(max_len, pad_value);
            p
        })
        .collect();
    (padded, max_len)
}

/// Convert padded 2D data into a Tensor<B, 2, Int>.
fn to_int_tensor<B: Backend>(data: &[Vec<i64>], device: &B::Device) -> Tensor<B, 2, Int> {
    let batch_size = data.len();
    let seq_len = data.first().map(|s| s.len()).unwrap_or(0);
    let flat: Vec<i64> = data.iter().flat_map(|row| row.iter().copied()).collect();
    let td = TensorData::new(flat, [batch_size, seq_len]);
    Tensor::from_data(td, device)
}

impl<B: Backend> Batcher<B, SimplificationItem, SimplificationBatch<B>>
    for SimplificationBatcher
{
    fn batch(&self, items: Vec<SimplificationItem>, device: &B::Device) -> SimplificationBatch<B> {
        let enc_seqs: Vec<Vec<i64>> = items.iter().map(|i| i.enc_ids.clone()).collect();
        let dec_in_seqs: Vec<Vec<i64>> = items.iter().map(|i| i.dec_input.clone()).collect();
        let dec_tgt_seqs: Vec<Vec<i64>> = items.iter().map(|i| i.dec_target.clone()).collect();

        let (enc_padded, _) = pad_sequences(&enc_seqs, 0);
        let (dec_in_padded, _) = pad_sequences(&dec_in_seqs, 0);
        let (dec_tgt_padded, _) = pad_sequences(&dec_tgt_seqs, 0);

        let enc_ids: Tensor<B, 2, Int> = to_int_tensor(&enc_padded, device);
        let dec_input: Tensor<B, 2, Int> = to_int_tensor(&dec_in_padded, device);
        let dec_target: Tensor<B, 2, Int> = to_int_tensor(&dec_tgt_padded, device);

        // Padding masks: true where padded (value == 0)
        let enc_pad_mask = enc_ids.clone().equal_elem(0);
        let dec_pad_mask = dec_input.clone().equal_elem(0);

        SimplificationBatch {
            enc_ids,
            enc_pad_mask,
            dec_input,
            dec_target,
            dec_pad_mask,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use burn::backend::NdArray;
    use erd_symbolic::random_search::IndexedRuleSet;
    use erd_symbolic::RuleSet;

    type TestBackend = NdArray;

    #[test]
    fn test_pad_sequences() {
        let seqs = vec![vec![1, 2, 3], vec![4, 5], vec![6]];
        let (padded, max_len) = pad_sequences(&seqs, 0);
        assert_eq!(max_len, 3);
        assert_eq!(padded[0], vec![1, 2, 3]);
        assert_eq!(padded[1], vec![4, 5, 0]);
        assert_eq!(padded[2], vec![6, 0, 0]);
    }

    #[test]
    fn test_load_data() {
        let data_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("data_training");
        if !data_dir.exists() {
            eprintln!("Skipping test_load_data: data_training/ not found");
            return;
        }
        let (train, val) = load_data(data_dir.to_str().unwrap(), 0.1, 42);
        let total = train.len() + val.len();
        assert!(total > 10_000, "expected >10K examples, got {}", total);
        let val_frac = val.len() as f64 / total as f64;
        assert!(
            (val_frac - 0.1).abs() < 0.02,
            "val fraction {} too far from 0.1",
            val_frac
        );
        println!(
            "Loaded {} train + {} val = {} total examples",
            train.len(),
            val.len(),
            total
        );
    }

    #[test]
    fn test_dataset_item_encoding() {
        let indexed = IndexedRuleSet::new(RuleSet::full());
        let enc_vocab = EncoderVocab::new(&indexed);
        let dec_vocab = DecoderVocab::new(&indexed, 64);

        let ex = TrainingExample {
            input_tokens: vec!["ADD".to_string(), "I_1".to_string(), "I_2".to_string()],
            actions: vec![erd_symbolic::training_data::TrainingAction {
                rule_direction: 2,
                position: 0,
            }],
            output_complexity: 1,
            input_complexity: 3,
        };

        let dataset = SimplificationDataset::new(&[ex], &enc_vocab, &dec_vocab, 50);
        assert_eq!(dataset.len(), 1);

        let item = dataset.get(0).unwrap();
        assert_eq!(item.enc_ids.len(), 3);
        assert!(
            item.enc_ids.iter().all(|&id| id > 0),
            "all tokens should be non-PAD"
        );

        // dec_target: [RULE_2, POS_0, STOP] = 3 tokens
        assert_eq!(item.dec_target.len(), 3);
        assert_eq!(*item.dec_target.last().unwrap(), DecoderVocab::STOP as i64);

        // dec_input: [BOS, RULE_2, POS_0] = 3 tokens
        assert_eq!(item.dec_input.len(), 3);
        assert_eq!(item.dec_input[0], DecoderVocab::BOS as i64);
    }

    #[test]
    fn test_batcher_shapes() {
        let indexed = IndexedRuleSet::new(RuleSet::full());
        let enc_vocab = EncoderVocab::new(&indexed);
        let dec_vocab = DecoderVocab::new(&indexed, 64);

        let examples = vec![
            TrainingExample {
                input_tokens: vec!["ADD".to_string(), "I_1".to_string(), "I_2".to_string()],
                actions: vec![erd_symbolic::training_data::TrainingAction {
                    rule_direction: 2,
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
                actions: vec![
                    erd_symbolic::training_data::TrainingAction {
                        rule_direction: 5,
                        position: 0,
                    },
                    erd_symbolic::training_data::TrainingAction {
                        rule_direction: 8,
                        position: 1,
                    },
                ],
                output_complexity: 2,
                input_complexity: 4,
            },
        ];

        let dataset = SimplificationDataset::new(&examples, &enc_vocab, &dec_vocab, 50);
        let items: Vec<_> = (0..dataset.len())
            .map(|i| dataset.get(i).unwrap())
            .collect();

        let batcher = SimplificationBatcher;
        let device = &burn::backend::ndarray::NdArrayDevice::Cpu;
        let batch: SimplificationBatch<TestBackend> = batcher.batch(items, device);

        // Batch size = 2
        let enc_shape = batch.enc_ids.shape();
        assert_eq!(enc_shape.dims[0], 2, "batch size");
        // Max encoder length = max(3, 4) = 4
        assert_eq!(enc_shape.dims[1], 4, "max enc length");

        // Decoder shapes
        let dec_in_shape = batch.dec_input.shape();
        let dec_tgt_shape = batch.dec_target.shape();
        assert_eq!(dec_in_shape.dims[0], 2);
        assert_eq!(dec_tgt_shape.dims[0], 2);
        assert_eq!(dec_in_shape.dims[1], dec_tgt_shape.dims[1]);

        // Mask shapes should match tensors
        assert_eq!(batch.enc_pad_mask.shape().dims, enc_shape.dims);
        assert_eq!(batch.dec_pad_mask.shape().dims, dec_in_shape.dims);
    }

    #[test]
    fn test_full_pipeline() {
        let data_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("data_training");
        if !data_dir.exists() {
            eprintln!("Skipping test_full_pipeline: data_training/ not found");
            return;
        }

        let indexed = IndexedRuleSet::new(RuleSet::full());
        let enc_vocab = EncoderVocab::new(&indexed);
        let dec_vocab = DecoderVocab::new(&indexed, 64);

        let (train, _val) = load_data(data_dir.to_str().unwrap(), 0.1, 42);
        let dataset = SimplificationDataset::new(&train, &enc_vocab, &dec_vocab, 50);

        println!("Dataset: {} items", dataset.len());
        assert!(dataset.len() > 10_000);

        // Batch the first 4 items
        let items: Vec<_> = (0..4).map(|i| dataset.get(i).unwrap()).collect();
        let batcher = SimplificationBatcher;
        let device = &burn::backend::ndarray::NdArrayDevice::Cpu;
        let batch: SimplificationBatch<TestBackend> = batcher.batch(items, device);

        assert_eq!(batch.enc_ids.shape().dims[0], 4);
        assert_eq!(batch.dec_input.shape().dims[0], 4);
        assert_eq!(batch.dec_target.shape().dims[0], 4);

        println!("Batch enc shape: {:?}", batch.enc_ids.shape());
        println!("Batch dec_input shape: {:?}", batch.dec_input.shape());
        println!("Batch dec_target shape: {:?}", batch.dec_target.shape());
    }
}
