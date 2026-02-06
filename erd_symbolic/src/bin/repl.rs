fn main() {
    if let Err(err) = erd_symbolic::repl::run() {
        eprintln!("repl error: {:?}", err);
    }
}
