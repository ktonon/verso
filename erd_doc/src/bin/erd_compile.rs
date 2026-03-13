use clap::Parser;
use erd_doc::compile_tex::compile_to_tex;
use erd_doc::parse::parse_document;
use std::fs;
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

    let src = match fs::read_to_string(&cli.file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {}: {}", cli.file, e);
            process::exit(1);
        }
    };

    let doc = match parse_document(&src) {
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
