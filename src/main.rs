fn main() {
    let exit_code = orangensaft::cli::run(std::env::args().collect());
    std::process::exit(exit_code);
}
