use erd_symbolic::training_data::TrainingExample;

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

    use rand::rngs::StdRng;
    use rand::seq::SliceRandom;
    use rand::SeedableRng;
    let mut rng = StdRng::seed_from_u64(seed);
    examples.shuffle(&mut rng);

    let split = (examples.len() as f64 * (1.0 - val_fraction)) as usize;
    let val = examples.split_off(split);
    (examples, val)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
