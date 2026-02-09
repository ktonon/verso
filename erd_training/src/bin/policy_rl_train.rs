use clap::Parser;
use erd_training::policy_rl_train::{PolicyRLConfig, policy_rl_train};

fn main() {
    let config = PolicyRLConfig::parse();

    match config.device.as_str() {
        "wgpu" => run_wgpu(config),
        _ => run_ndarray(config),
    }
}

fn run_ndarray(config: PolicyRLConfig) {
    type B = burn::backend::Autodiff<burn::backend::NdArray>;
    policy_rl_train::<B>(config, Default::default());
}

fn run_wgpu(config: PolicyRLConfig) {
    type B = burn::backend::Autodiff<burn::backend::Wgpu>;
    policy_rl_train::<B>(config, Default::default());
}
