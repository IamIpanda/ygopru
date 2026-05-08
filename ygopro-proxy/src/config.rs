use std::net::SocketAddr;

use once_cell::sync::Lazy;
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

pub static CONFIG: Lazy<Config> = Lazy::new(|| Config::parse());
