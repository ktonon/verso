fn main() {
    if let Err(err) = ogma_symbolic::repl::run() {
        eprintln!("repl error: {:?}", err);
    }
}
