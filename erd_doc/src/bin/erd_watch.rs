use clap::Parser;
use erd_doc::parse::parse_document;
use erd_doc::report::ReportFormatter;
use erd_doc::verify::verify_document;
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use std::fs;
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "erd_watch", about = "Watch .erd files and re-verify on save")]
struct Cli {
    /// .erd files to watch
    #[arg(required = true)]
    files: Vec<String>,
}

fn check_files(files: &[String]) {
    // Clear screen
    print!("\x1b[2J\x1b[H");

    let mut all_passed = true;

    for path in files {
        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: {}: {}", path, e);
                all_passed = false;
                continue;
            }
        };

        let doc = match parse_document(&src) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("error: {}: {}", path, e);
                all_passed = false;
                continue;
            }
        };

        let report = verify_document(&doc);
        let formatter = ReportFormatter {
            report: &report,
            filename: path,
        };
        print!("{}", formatter);

        if !report.all_passed() {
            all_passed = false;
        }
    }

    if all_passed {
        println!("\n\x1b[32mWatching for changes... (Ctrl+C to stop)\x1b[0m");
    } else {
        println!("\n\x1b[31mWatching for changes... (Ctrl+C to stop)\x1b[0m");
    }
}

fn main() {
    let cli = Cli::parse();

    // Initial check
    check_files(&cli.files);

    // Set up file watcher
    let (tx, rx) = mpsc::channel();
    let mut debouncer = new_debouncer(Duration::from_millis(300), tx)
        .expect("failed to create file watcher");

    for path in &cli.files {
        debouncer
            .watcher()
            .watch(
                Path::new(path),
                notify::RecursiveMode::NonRecursive,
            )
            .unwrap_or_else(|e| {
                eprintln!("warning: cannot watch {}: {}", path, e);
            });
    }

    // Watch loop
    loop {
        match rx.recv() {
            Ok(Ok(events)) => {
                // Only re-check if at least one event is a write
                let has_write = events
                    .iter()
                    .any(|e| matches!(e.kind, DebouncedEventKind::Any));
                if has_write {
                    check_files(&cli.files);
                }
            }
            Ok(Err(e)) => {
                eprintln!("watch error: {}", e);
            }
            Err(_) => {
                // Channel closed — watcher dropped
                break;
            }
        }
    }
}
