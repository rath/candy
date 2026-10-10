mod cli;
mod gui;
mod terminal;
use clap::Parser;
fn main() {
    if let Err(error) = cli::run(cli::Cli::parse()) {
        eprintln!("candy: {error:#}");
        std::process::exit(1);
    }
}
