use std::io::{self, Write};

use clap::Parser;
use workroot::{Cli, run};

fn main() {
    let cli = Cli::parse();

    let exit_code = match run(cli) {
        Ok(Some(output)) => match io::stdout().write_all(output.as_bytes()) {
            Ok(()) => 0,
            Err(error) if error.kind() == io::ErrorKind::BrokenPipe => 0,
            Err(error) => {
                eprintln!("error: I/O failed: {error}");
                9
            }
        },
        Ok(None) => 0,
        Err(error) => {
            eprintln!("error: {error}");
            error.exit_code()
        }
    };

    std::process::exit(exit_code);
}
