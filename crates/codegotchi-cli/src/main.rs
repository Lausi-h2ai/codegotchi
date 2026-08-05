fn main() {
    if let Err(error) = codegotchi_cli::cli::run(std::env::args().skip(1)) {
        eprintln!("codegotchi: {error}");
        std::process::exit(2);
    }
}
