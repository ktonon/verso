use clap::Parser;
use verso_doc::parse::parse_document_from_file;
use verso_doc::report::ReportFormatter;
use verso_doc::verify::verify_document;
use std::path::Path;
use std::process;

#[derive(Parser)]
#[command(name = "verso_check", about = "Verify claims in .verso documents")]
struct Cli {
    /// .verso files to check
    #[arg(required = true)]
    files: Vec<String>,
}

fn main() {
    let cli = Cli::parse();
    let mut all_passed = true;

    for file in &cli.files {
        let doc = match parse_document_from_file(Path::new(file)) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("error: {}: {}", file, e);
                all_passed = false;
                continue;
            }
        };

        let report = verify_document(&doc);
        let formatter = ReportFormatter {
            report: &report,
            filename: file,
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
