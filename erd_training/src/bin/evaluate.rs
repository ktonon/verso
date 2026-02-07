use clap::Parser;
use erd_training::config::EvalConfig;
use erd_training::dataset::load_data;
use erd_training::evaluate::{evaluate, print_metrics};
use erd_training::train::load_model;
use erd_training::vocab::{DecoderVocab, EncoderVocab};

use erd_symbolic::random_search::IndexedRuleSet;
use erd_symbolic::RuleSet;

fn main() {
    let config = EvalConfig::parse();

    match config.device.as_str() {
        "wgpu" => run_wgpu(config),
        _ => run_ndarray(config),
    }
}

fn run_ndarray(config: EvalConfig) {
    type B = burn::backend::NdArray;
    run::<B>(config, Default::default());
}

fn run_wgpu(config: EvalConfig) {
    type B = burn::backend::Wgpu;
    run::<B>(config, Default::default());
}

fn run<B: burn::prelude::Backend>(config: EvalConfig, device: B::Device) {
    let indexed = IndexedRuleSet::new(RuleSet::full());
    let enc_vocab = EncoderVocab::new(&indexed);
    let dec_vocab = DecoderVocab::new(&indexed, config.max_positions);

    println!("Loading data from {}...", config.data_dir);
    let (_train, val) = load_data(&config.data_dir, config.val_fraction, config.seed);
    println!("Validation examples: {}", val.len());

    println!("Loading model from {}...", config.checkpoint);
    let train_config = config.to_train_config();
    let model: erd_training::model::SimplificationModel<B> =
        load_model(&train_config, enc_vocab.size(), dec_vocab.size(), &device, &config.checkpoint);

    println!("Evaluating...");
    let t0 = std::time::Instant::now();
    let metrics = evaluate(
        &model,
        &val,
        &enc_vocab,
        &dec_vocab,
        &indexed,
        config.batch_size,
        config.invalid_penalty,
        &device,
    );
    let elapsed = t0.elapsed();

    print_metrics(&metrics);
    println!("Evaluation time:      {:.1}s", elapsed.as_secs_f64());
}
