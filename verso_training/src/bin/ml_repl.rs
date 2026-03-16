use burn::backend::ndarray::NdArray;
use burn::backend::wgpu::Wgpu;
use clap::Parser;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

use verso_symbolic::dim::Dimension;
use verso_symbolic::fmt::fmt_colored;
use verso_symbolic::parse_expr;
use verso_symbolic::unit::base_si_display;
use verso_symbolic::{simplify_with_trace, Expr, RuleSet, TraceStep};
use verso_training::ml_simplify::MLSimplifier;

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "checkpoints/policy_rl_best")]
    checkpoint: String,
    #[arg(long, default_value = "ndarray")]
    device: String,
    /// Show the resolver engine used (beam/ml) after each result
    #[arg(long, short)]
    verbose: bool,
}

fn main() {
    let args = Args::parse();
    match args.device.as_str() {
        "wgpu" => run::<Wgpu>(&args),
        _ => run::<NdArray>(&args),
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum Mode {
    Hybrid,
    MlOnly,
    BeamOnly,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum HistoryMode {
    Results,
    Inputs,
}

fn run<B: burn::prelude::Backend>(args: &Args) {
    println!("Loading model from {}...", args.checkpoint);
    let device = B::Device::default();
    let simplifier = MLSimplifier::<B>::load(&args.checkpoint, device);
    println!("Ready.\n");

    let mut rl = DefaultEditor::new().expect("Failed to create editor");
    let mut show_trace = false;
    let mut show_debug = false;
    let mut mode = Mode::Hybrid;
    let mut history_mode = HistoryMode::Inputs;
    let mut input_history: Vec<String> = Vec::new();
    let mut result_history: Vec<String> = Vec::new();

    loop {
        match rl.readline("> ") {
            Ok(line) => {
                let input = line.trim();
                if input.is_empty() {
                    continue;
                }
                if input == ":q" || input == ":quit" || input == ":exit" {
                    break;
                }
                if input == ":trace" {
                    show_trace = !show_trace;
                    println!("trace: {}\n", if show_trace { "on" } else { "off" });
                    continue;
                }
                if input == ":debug" {
                    show_debug = !show_debug;
                    println!("debug: {}\n", if show_debug { "on" } else { "off" });
                    continue;
                }
                if input == ":ml" {
                    mode = Mode::MlOnly;
                    println!("mode: ml only\n");
                    continue;
                }
                if input == ":beam" {
                    mode = Mode::BeamOnly;
                    println!("mode: beam search only\n");
                    continue;
                }
                if input == ":hybrid" {
                    mode = Mode::Hybrid;
                    println!("mode: hybrid (ml + beam fallback)\n");
                    continue;
                }
                if input == ":history" || input == ":hist" {
                    history_mode = match history_mode {
                        HistoryMode::Results => HistoryMode::Inputs,
                        HistoryMode::Inputs => HistoryMode::Results,
                    };
                    match history_mode {
                        HistoryMode::Results => {
                            reload_history(&mut rl, &result_history);
                            println!("history: results\n");
                        }
                        HistoryMode::Inputs => {
                            reload_history(&mut rl, &input_history);
                            println!("history: inputs\n");
                        }
                    }
                    continue;
                }

                record_input(&mut input_history, &mut rl, history_mode, input);

                match parse_expr(input) {
                    Ok(expr) => {
                        let input_dim = expr.first_unit().map(|u| u.dimension.clone());
                        let (simplified, trace, engine) = match mode {
                            Mode::Hybrid => {
                                if show_debug {
                                    let (ml_result, ml_trace) = simplifier.run_inference(&expr);
                                    let valid = ml_result.valid_steps;
                                    let total = ml_result.total_steps;
                                    let in_c = ml_result.input_complexity;
                                    let out_c = ml_result.final_complexity;
                                    let improved = out_c < in_c;
                                    println!(
                                        "\x1b[90m  ml: {}/{} valid, complexity {} → {} {}\x1b[0m",
                                        valid,
                                        total,
                                        in_c,
                                        out_c,
                                        if improved { "(accepted)" } else { "(rejected)" }
                                    );
                                    if !ml_trace.is_empty() && show_trace {
                                        println!("\x1b[90m  ml trace:\x1b[0m");
                                        print_trace(&ml_trace);
                                    }
                                    if improved {
                                        (ml_result.final_expr, ml_trace, "ml")
                                    } else {
                                        let (s, t) = simplify_with_trace(&expr, &RuleSet::full());
                                        (s, t, "beam")
                                    }
                                } else {
                                    let (s, t, used_ml) = simplifier.simplify(&expr);
                                    (s, t, if used_ml { "ml" } else { "beam" })
                                }
                            }
                            Mode::MlOnly => {
                                let (ml_result, ml_trace) = simplifier.run_inference(&expr);
                                if show_debug {
                                    let valid = ml_result.valid_steps;
                                    let total = ml_result.total_steps;
                                    let in_c = ml_result.input_complexity;
                                    let out_c = ml_result.final_complexity;
                                    println!(
                                        "\x1b[90m  ml: {}/{} valid, complexity {} → {}\x1b[0m",
                                        valid, total, in_c, out_c,
                                    );
                                    if !ml_trace.is_empty() && show_trace {
                                        println!("\x1b[90m  ml trace:\x1b[0m");
                                        print_trace(&ml_trace);
                                    }
                                }
                                if ml_result.final_complexity < ml_result.input_complexity {
                                    (ml_result.final_expr, ml_trace, "ml")
                                } else {
                                    println!("(ml: no improvement)\n");
                                    record_result(
                                        &mut result_history,
                                        &mut rl,
                                        history_mode,
                                        &expr,
                                    );
                                    continue;
                                }
                            }
                            Mode::BeamOnly => {
                                let (s, t) = simplify_with_trace(&expr, &RuleSet::full());
                                (s, t, "beam")
                            }
                        };

                        if show_trace {
                            print_trace(&trace);
                        }

                        let unit_suffix = format_unit_suffix(&simplified, input_dim.as_ref());
                        let engine_suffix = if args.verbose {
                            format!("  \x1b[90m[{}]\x1b[0m", engine)
                        } else {
                            String::new()
                        };
                        println!(
                            "{}{}{}\n",
                            fmt_colored(&simplified),
                            unit_suffix,
                            engine_suffix
                        );
                        record_result(&mut result_history, &mut rl, history_mode, &simplified);
                    }
                    Err(err) => {
                        println!("Error: {:?}\n", err);
                    }
                }
            }
            Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => break,
            Err(_) => break,
        }
    }
}

/// Build a unit suffix for the simplified result.
///
/// When the input had unit annotations (Quantity nodes), the simplifier folds
/// them into plain rationals by multiplying in the SI scale. We recover the
/// base SI unit from the input dimension and display it.
/// E.g., `4 [km]` → simplified `4000` → displayed `4000 [m]`.
fn format_unit_suffix(simplified: &Expr, input_dim: Option<&Dimension>) -> String {
    let dim = match input_dim {
        Some(d) => d,
        None => return String::new(),
    };
    // If the simplified expr still has units, the formatter shows them already
    if simplified.first_unit().is_some() {
        return String::new();
    }
    format!(" \x1b[36m[{}]\x1b[0m", base_si_display(dim))
}

fn print_trace(trace: &[TraceStep]) {
    let plain_widths: Vec<usize> = trace
        .iter()
        .map(|s| format!("{}", s.expr).chars().count())
        .collect();
    let max_expr_width = plain_widths.iter().copied().max().unwrap_or(0);
    let max_name_width = trace
        .iter()
        .filter_map(|s| s.rule_name.as_ref().map(|n| n.len()))
        .max()
        .unwrap_or(0);

    for (i, step) in trace.iter().enumerate() {
        let expr_str = fmt_colored(&step.expr);
        let padding = max_expr_width - plain_widths[i];
        match (&step.rule_name, &step.rule_display) {
            (Some(name), Some(display)) => {
                println!(
                    "{}: {}{:padding$}  \x1b[90m{:width$}\x1b[0m  \x1b[2m{}\x1b[0m",
                    i,
                    expr_str,
                    "",
                    name,
                    display,
                    padding = padding,
                    width = max_name_width,
                );
            }
            (Some(name), None) => {
                println!(
                    "{}: {}{:padding$}  \x1b[90m{}\x1b[0m",
                    i,
                    expr_str,
                    "",
                    name,
                    padding = padding,
                );
            }
            _ => println!("{}: {}", i, expr_str),
        }
    }
}

fn record_input(
    input_history: &mut Vec<String>,
    rl: &mut DefaultEditor,
    history_mode: HistoryMode,
    input: &str,
) {
    input_history.push(input.to_string());
    if history_mode == HistoryMode::Inputs {
        let _ = rl.add_history_entry(input);
    }
}

fn record_result(
    result_history: &mut Vec<String>,
    rl: &mut DefaultEditor,
    history_mode: HistoryMode,
    simplified: &verso_symbolic::Expr,
) {
    let rendered = format!("{}", simplified);
    result_history.push(rendered.clone());
    if history_mode == HistoryMode::Results {
        let _ = rl.add_history_entry(rendered);
    }
}

fn reload_history(rl: &mut DefaultEditor, entries: &[String]) {
    let _ = rl.clear_history();
    for entry in entries {
        let _ = rl.add_history_entry(entry);
    }
}
