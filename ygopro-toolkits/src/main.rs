use std::net::SocketAddr;
use std::path::PathBuf;

use clap::{Parser, Subcommand};

mod proxy;
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
        /// Path to the replay (.yrp) file
        path: PathBuf,
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
        Commands::ValidateReplay { path } => {
            match validate_replay::validate_replay(&path).await {
                Ok(summary) => {
                    println!("replay is valid: {} responses replayed", summary.response_count);
                    match summary.winner {
                        Some(winner) => println!("winner: {winner:?}"),
                        None => println!("winner: draw"),
                    }
                }
                Err(error) => {
                    eprintln!("replay is invalid: {error}");
                    std::process::exit(1);
                }
            }
        }
        Commands::Proxy { target, port } => {
            proxy::run_proxy(target, port).await;
        }
    }
}
