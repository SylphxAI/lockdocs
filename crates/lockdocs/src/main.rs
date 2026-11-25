mod cli;
mod login;
mod mcp;
mod preview;
mod pro;
mod setup;
mod tools;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() {
    if let Err(e) = cli::run() {
        eprintln!("lockdocs: {e:#}");
        std::process::exit(1);
    }
}
