use std::io;

use clap::Parser;

mod cli;

fn main() -> io::Result<()> {
    let args = cli::Cli::parse();
    cli::run(args.command, args.format)
}
