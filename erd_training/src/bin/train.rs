use clap::Parser;
use erd_training::config::TrainConfig;
use erd_training::train::supervised_train;

fn main() {
    let config = TrainConfig::parse();

    match config.device.as_str() {
        "wgpu" => run_wgpu(config),
        _ => run_ndarray(config),
    }
}

fn run_ndarray(config: TrainConfig) {
    type B = burn::backend::Autodiff<burn::backend::NdArray>;
    supervised_train::<B>(config, Default::default());
}

fn run_wgpu(config: TrainConfig) {
    type B = burn::backend::Autodiff<burn::backend::Wgpu>;
    supervised_train::<B>(config, Default::default());
}
