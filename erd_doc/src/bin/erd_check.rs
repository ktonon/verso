use clap::Parser;
use erd_doc::parse::parse_document;
use erd_doc::report::ReportFormatter;
use erd_doc::verify::verify_document;
use std::fs;
use std::process;

#[derive(Parser)]
#[command(name = "erd_check", about = "Verify claims in .erd documents")]
struct Cli {
    /// .erd files to check
    #[arg(required = true)]
    files: Vec<String>,
}

fn main() {
    let cli = Cli::parse();
    let mut all_passed = true;

    for path in &cli.files {
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

    if !all_passed {
        process::exit(1);
    }
}
