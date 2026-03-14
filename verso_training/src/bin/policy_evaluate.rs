use clap::Parser;
use verso_training::policy_evaluate::{PolicyEvalConfig, run_policy_evaluation};

fn main() {
    let config = PolicyEvalConfig::parse();

    match config.device.as_str() {
        "wgpu" => run_wgpu(config),
        _ => run_ndarray(config),
    }
}

fn run_ndarray(config: PolicyEvalConfig) {
    type B = burn::backend::NdArray;
    run_policy_evaluation::<B>(config, Default::default());
}

fn run_wgpu(config: PolicyEvalConfig) {
    type B = burn::backend::Wgpu;
    run_policy_evaluation::<B>(config, Default::default());
}
