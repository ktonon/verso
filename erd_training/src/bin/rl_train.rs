use clap::Parser;
use erd_training::config::RLConfig;
use erd_training::rl_train::rl_train;

fn main() {
    let config = RLConfig::parse();

    match config.device.as_str() {
        "wgpu" => run_wgpu(config),
        _ => run_ndarray(config),
    }
}

fn run_ndarray(config: RLConfig) {
    type B = burn::backend::Autodiff<burn::backend::NdArray>;
    rl_train::<B>(config, Default::default());
}

fn run_wgpu(config: RLConfig) {
    type B = burn::backend::Autodiff<burn::backend::Wgpu>;
    rl_train::<B>(config, Default::default());
}
