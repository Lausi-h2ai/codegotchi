fn main() {
    match codegotchi_cli::cli::run_os(std::env::args_os().skip(1)) {
        Ok(status) => std::process::exit(status),
        Err(error) => {
            eprintln!("codegotchi: {error}");
            std::process::exit(2);
        }
    }
}
