//! `idbseqd`: the idbseq HTTP API server.

use std::process::ExitCode;

use clap::Parser;
use idbseq_server::cli::{Cli, Command, run};
use idbseq_server::config::Config;
use idbseq_server::serve;

fn main() -> ExitCode {
    let cli = Cli::parse();
    let config = match Config::load(&|key| std::env::var(key).ok()) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("idbseqd: {error}");
            return ExitCode::from(2);
        }
    };
    match cli.command.unwrap_or(Command::Serve) {
        Command::Serve => serve_forever(config),
        command => match run(command, &config) {
            Ok(message) => {
                println!("{message}");
                ExitCode::SUCCESS
            }
            Err(failure) => {
                eprintln!("idbseqd: {}", failure.message);
                ExitCode::from(u8::try_from(failure.code).unwrap_or(1))
            }
        },
    }
}

fn serve_forever(config: Config) -> ExitCode {
    serve::init_logging(&config.log);
    let admin_password = std::env::var("IDBSEQ_ADMIN_PASSWORD").ok();
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("idbseqd: cannot start runtime: {error}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(serve::run(config, admin_password)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(%error, "idbseqd failed");
            eprintln!("idbseqd: {error}");
            ExitCode::FAILURE
        }
    }
}
