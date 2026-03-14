use clap::Parser;
use verso_doc::compile_tex::compile_to_tex;
use verso_doc::parse::parse_document_from_file;
use std::fs;
use std::path::Path;
use std::process;

#[derive(Parser)]
#[command(name = "erd_compile", about = "Compile .erd documents to LaTeX")]
struct Cli {
    /// .erd file to compile
    file: String,

    /// Output file (default: stdout)
    #[arg(short, long)]
    output: Option<String>,
}

fn main() {
    let cli = Cli::parse();
    let path = Path::new(&cli.file);

    let doc = match parse_document_from_file(path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: {}: {}", cli.file, e);
            process::exit(1);
        }
    };

    let tex = compile_to_tex(&doc);

    if let Some(output_path) = &cli.output {
        if let Err(e) = fs::write(output_path, &tex) {
            eprintln!("error writing {}: {}", output_path, e);
            process::exit(1);
        }
        eprintln!("wrote {}", output_path);
    } else {
        print!("{}", tex);
    }
}
