use std::net::SocketAddr;
use std::sync::LazyLock;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Config {
    /// Proxy target
    #[arg(short, long)]
    pub target: SocketAddr,
    /// Proxy listening on port
    #[arg(short, long, default_value_t=8911)]
    pub port: u32
}

pub static CONFIG: LazyLock<Config> = LazyLock::new(|| Config::parse());
