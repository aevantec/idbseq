//! `idbseq`: command-line client for the idbseq HTTP API.

use std::process::ExitCode;

use clap::Parser;
use idbseq_cli::cli::Cli;
use idbseq_cli::prompt::Terminal;

fn main() -> ExitCode {
    let cli = Cli::parse();
    let env = |name: &str| std::env::var(name).ok();
    let status = idbseq_cli::run(
        cli,
        &env,
        &mut Terminal,
        &mut std::io::stdout().lock(),
        &mut std::io::stderr().lock(),
    );
    ExitCode::from(status)
}
