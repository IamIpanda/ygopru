use std::net::SocketAddr;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use glob::glob;

mod proxy;
mod start_game;
mod validate_replay;

#[derive(Parser)]
#[command(name = "ygopro-toolkits")]
#[command(about = "CLI tools for ygopro")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Validate a replay file by replaying it through the engine.
    ValidateReplay {
        /// Replay (.yrp) files to validate
        #[arg(required = true)]
        path: Vec<PathBuf>,
        /// Wait for a viewer to connect on this port before replaying responses
        #[arg(long)]
        wait: Option<u16>,
        /// Validation timeout in seconds
        #[arg(long, default_value_t = 5)]
        timeout: u64,
    },
    /// Logging proxy middleware.
    Proxy {
        /// Proxy target
        #[arg(short, long)]
        target: SocketAddr,
        /// Proxy listening on port
        #[arg(short, long, default_value_t = 8911)]
        port: u32,
    },
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::ValidateReplay { path, wait, timeout } => {
            let mut paths = Vec::new();
            for pattern in &path {
                match glob(&pattern.to_string_lossy()) {
                    Ok(matches) => paths.extend(matches.filter_map(Result::ok)),
                    Err(error) => {
                        log::error!("cannot parse pattern {}: {error}", pattern.display());
                        std::process::exit(1);
                    }
                }
            }
            if paths.is_empty() {
                log::error!("no replay files matched");
                std::process::exit(1);
            }
            let single_file = paths.len() == 1;
            let mut failed_count = 0;
            for path in paths {
                let wait = if single_file { wait } else { None };
                match validate_replay::validate_replay(&path, wait, timeout).await {
                    Ok(summary) => {
                        let winner_text = match summary.winner {
                            Some(winner) => format!("{winner:?}"),
                            None if summary.replayed_to_end => "unknown (surrendered)".to_string(),
                            None => "draw".to_string(),
                        };
                        log::info!(
                            "{}: replay is valid: {} responses replayed, winner: {winner_text}",
                            path.display(),
                            summary.response_count
                        );
                    }
                    Err(error) => {
                        log::warn!("{}: replay is invalid", path.display());
                        log::debug!("{}: {error}", path.display());
                        failed_count += 1;
                    }
                }
            }
            if failed_count > 0 {
                std::process::exit(1);
            }
        }
        Commands::Proxy { target, port } => {
            proxy::run_proxy(target, port).await;
        }
    }
}
