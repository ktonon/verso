use verso_symbolic::gen_expr::{gen_expr, GenExprConfig};
use verso_symbolic::random_search::{IndexedRuleSet, RandomizedBeamSearch};
use verso_symbolic::training_data::{
    build_vocab_metadata, search_run_to_example, write_jsonl, write_vocab_json,
};
use verso_symbolic::RuleSet;
use rand::rngs::StdRng;
use rand::SeedableRng;
use rayon::prelude::*;
use std::fs::File;
use std::io::BufWriter;
use std::sync::atomic::{AtomicUsize, Ordering};

struct CliArgs {
    output: Option<String>,
    count: usize,
    seed: u64,
    max_depth: usize,
    num_vars: usize,
    min_complexity: usize,
    max_complexity: usize,
    search_runs: usize,
    beam_width: usize,
    max_steps: usize,
    epsilon: f64,
}

impl Default for CliArgs {
    fn default() -> Self {
        CliArgs {
            output: None,
            count: 0,
            seed: 42,
            max_depth: 5,
            num_vars: 3,
            min_complexity: 3,
            max_complexity: usize::MAX,
            search_runs: 5,
            beam_width: 20,
            max_steps: 200,
            epsilon: 0.3,
        }
    }
}

impl CliArgs {
    fn default_output_path(&self) -> String {
        let max_c = if self.max_complexity == usize::MAX {
            "inf".to_string()
        } else {
            self.max_complexity.to_string()
        };
        format!(
            "data_training/d{}_v{}_c{}-{}_n{}_s{}.jsonl",
            self.max_depth, self.num_vars, self.min_complexity, max_c, self.count, self.seed,
        )
    }

    fn output_path(&self) -> String {
        self.output
            .clone()
            .unwrap_or_else(|| self.default_output_path())
    }
}

fn parse_args() -> Result<CliArgs, String> {
    let mut args = CliArgs::default();
    let mut iter = std::env::args().skip(1);

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--output" => {
                args.output = Some(
                    iter.next()
                        .ok_or("--output requires a value".to_string())?,
                );
            }
            "--count" => {
                args.count = iter
                    .next()
                    .ok_or("--count requires a value".to_string())?
                    .parse()
                    .map_err(|e| format!("--count: {}", e))?;
            }
            "--seed" => {
                args.seed = iter
                    .next()
                    .ok_or("--seed requires a value".to_string())?
                    .parse()
                    .map_err(|e| format!("--seed: {}", e))?;
            }
            "--max-depth" => {
                args.max_depth = iter
                    .next()
                    .ok_or("--max-depth requires a value".to_string())?
                    .parse()
                    .map_err(|e| format!("--max-depth: {}", e))?;
            }
            "--num-vars" => {
                args.num_vars = iter
                    .next()
                    .ok_or("--num-vars requires a value".to_string())?
                    .parse()
                    .map_err(|e| format!("--num-vars: {}", e))?;
            }
            "--search-runs" => {
                args.search_runs = iter
                    .next()
                    .ok_or("--search-runs requires a value".to_string())?
                    .parse()
                    .map_err(|e| format!("--search-runs: {}", e))?;
            }
            "--beam-width" => {
                args.beam_width = iter
                    .next()
                    .ok_or("--beam-width requires a value".to_string())?
                    .parse()
                    .map_err(|e| format!("--beam-width: {}", e))?;
            }
            "--max-steps" => {
                args.max_steps = iter
                    .next()
                    .ok_or("--max-steps requires a value".to_string())?
                    .parse()
                    .map_err(|e| format!("--max-steps: {}", e))?;
            }
            "--min-complexity" => {
                args.min_complexity = iter
                    .next()
                    .ok_or("--min-complexity requires a value".to_string())?
                    .parse()
                    .map_err(|e| format!("--min-complexity: {}", e))?;
            }
            "--max-complexity" => {
                args.max_complexity = iter
                    .next()
                    .ok_or("--max-complexity requires a value".to_string())?
                    .parse()
                    .map_err(|e| format!("--max-complexity: {}", e))?;
            }
            "--epsilon" => {
                args.epsilon = iter
                    .next()
                    .ok_or("--epsilon requires a value".to_string())?
                    .parse()
                    .map_err(|e| format!("--epsilon: {}", e))?;
            }
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            other => {
                return Err(format!("unknown argument: {}", other));
            }
        }
    }

    if args.count == 0 {
        return Err("--count is required and must be > 0".to_string());
    }
    Ok(args)
}

fn print_usage() {
    eprintln!(
        "Usage: gen_data --count N [options]

Options:
  --output PATH         Output JSONL file (default: data_training/<params>.jsonl)
  --count N             Number of expressions to attempt (required)
  --seed N              Master seed (default 42)
  --max-depth N         Expression max depth (default 5)
  --num-vars N          Distinct variables (default 3)
  --min-complexity N    Skip expressions with complexity < N (default 3)
  --max-complexity N    Skip expressions with complexity > N (default unlimited)
  --search-runs N       Beam search runs per expression (default 5)
  --beam-width N        Beam width (default 20)
  --max-steps N         Max search steps (default 200)
  --epsilon F           Epsilon-greedy fraction (default 0.3)"
    );
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Error: {}", e);
            print_usage();
            std::process::exit(1);
        }
    };

    let output_path = args.output_path();

    let max_c_display = if args.max_complexity == usize::MAX {
        "unlimited".to_string()
    } else {
        args.max_complexity.to_string()
    };
    eprintln!(
        "Generating training data: count={}, seed={}, max_depth={}, num_vars={}, complexity={}..{}, search_runs={}",
        args.count, args.seed, args.max_depth, args.num_vars, args.min_complexity, max_c_display, args.search_runs
    );
    eprintln!("Output: {}", output_path);

    let rules = IndexedRuleSet::new(RuleSet::full());
    let search = RandomizedBeamSearch {
        beam_width: args.beam_width,
        max_steps: args.max_steps,
        epsilon: args.epsilon,
        shuffle_rules: true,
    };
    let gen_config = GenExprConfig {
        max_depth: args.max_depth,
        num_vars: args.num_vars,
        ..Default::default()
    };

    let generated = AtomicUsize::new(0);
    let skipped = AtomicUsize::new(0);

    let examples: Vec<_> = (0..args.count)
        .into_par_iter()
        .filter_map(|i| {
            let expr_seed = args.seed.wrapping_add(i as u64);
            let mut rng = StdRng::seed_from_u64(expr_seed);
            let expr = gen_expr(&mut rng, &gen_config);

            // Skip expressions outside the complexity range
            let c = expr.complexity();
            if c < args.min_complexity || c > args.max_complexity {
                skipped.fetch_add(1, Ordering::Relaxed);
                return None;
            }

            // Use a different seed offset for search to avoid correlation
            let search_seed = args.seed.wrapping_add(1_000_000 + i as u64);
            let best = search.search_best(&expr, &rules, args.search_runs, search_seed);

            // Skip if no simplification found
            if best.trace.is_empty() || best.final_complexity >= expr.complexity() {
                skipped.fetch_add(1, Ordering::Relaxed);
                return None;
            }

            let example = search_run_to_example(&best);
            if example.is_none() {
                skipped.fetch_add(1, Ordering::Relaxed);
                return None;
            }

            let count = generated.fetch_add(1, Ordering::Relaxed) + 1;
            if count % 100 == 0 {
                eprintln!(
                    "  Generated {}/{} examples ({} skipped)",
                    count,
                    args.count,
                    skipped.load(Ordering::Relaxed)
                );
            }
            example
        })
        .collect();

    // Ensure output directory exists
    if let Some(parent) = std::path::Path::new(&output_path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).unwrap_or_else(|e| {
                eprintln!("Failed to create directory {}: {}", parent.display(), e);
                std::process::exit(1);
            });
        }
    }

    // Write vocab.json alongside the data
    let vocab_path = std::path::Path::new(&output_path)
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join("vocab.json");
    let vocab = build_vocab_metadata(&rules);
    let vocab_file = File::create(&vocab_path).unwrap_or_else(|e| {
        eprintln!("Failed to create {}: {}", vocab_path.display(), e);
        std::process::exit(1);
    });
    let mut vocab_writer = BufWriter::new(vocab_file);
    write_vocab_json(&vocab, &mut vocab_writer).unwrap_or_else(|e| {
        eprintln!("Failed to write vocab.json: {}", e);
        std::process::exit(1);
    });
    eprintln!(
        "Vocab: {} encoder tokens, {} rule directions → {}",
        vocab.encoder_tokens.len(),
        vocab.total_directions,
        vocab_path.display()
    );

    // Write JSONL output
    let file = File::create(&output_path).unwrap_or_else(|e| {
        eprintln!("Failed to create {}: {}", output_path, e);
        std::process::exit(1);
    });
    let mut writer = BufWriter::new(file);
    write_jsonl(&examples, &mut writer).unwrap_or_else(|e| {
        eprintln!("Failed to write JSONL: {}", e);
        std::process::exit(1);
    });

    eprintln!(
        "Done: {} examples written to {} ({} skipped)",
        examples.len(),
        output_path,
        skipped.load(Ordering::Relaxed)
    );
}
