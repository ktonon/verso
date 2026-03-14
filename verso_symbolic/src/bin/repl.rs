fn main() {
    if let Err(err) = verso_symbolic::repl::run() {
        eprintln!("repl error: {:?}", err);
    }
}
