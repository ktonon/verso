//! Public API for the training and learned-simplification components.
//!
//! The crate root groups the main data, evaluation, and training entry points so
//! callers do not have to discover them by reading each module individually.

pub mod config;
pub mod dataset;
pub mod evaluate;
pub mod ml_simplify;
pub mod policy_dataset;
pub mod policy_evaluate;
pub mod policy_model;
pub mod policy_rl_train;
pub mod policy_train;
pub mod schedule;
pub mod vocab;

pub use config::PolicyConfig;
pub use dataset::load_data;
pub use evaluate::{compute_metrics, compute_reward, print_metrics, tokens_to_expr, EvalMetrics};
pub use ml_simplify::MLSimplifier;
pub use policy_dataset::{PolicyBatch, PolicyBatcher, PolicyDataset, PolicyItem};
pub use policy_evaluate::{
    policy_evaluate, policy_inference_loop, run_policy_evaluation, PolicyEvalConfig,
};
pub use policy_model::PolicyModel;
pub use policy_rl_train::{policy_rl_train, PolicyRLConfig};
pub use policy_train::{
    load_policy_model, policy_supervised_train, save_policy_checkpoint, PolicyTrainConfig,
};
pub use schedule::cosine_lr;
pub use vocab::EncoderVocab;
