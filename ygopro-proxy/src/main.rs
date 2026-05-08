mod proxy;
mod config;

#[tokio::main]
async fn main() {
    pretty_env_logger::init();
    proxy::run_proxy().await;
}
