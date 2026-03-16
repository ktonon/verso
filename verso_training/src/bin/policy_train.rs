use clap::Parser;
use verso_training::policy_train::{policy_supervised_train, PolicyTrainConfig};

fn main() {
    let config = PolicyTrainConfig::parse();

    match config.device.as_str() {
        "wgpu" => run_wgpu(config),
        _ => run_ndarray(config),
    }
}

fn run_ndarray(config: PolicyTrainConfig) {
    type B = burn::backend::Autodiff<burn::backend::NdArray>;
    policy_supervised_train::<B>(config, Default::default());
}

fn run_wgpu(config: PolicyTrainConfig) {
    type B = burn::backend::Autodiff<burn::backend::Wgpu>;
    policy_supervised_train::<B>(config, Default::default());
}
