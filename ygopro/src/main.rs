use ygopro::cli::*;

#[tokio::main]
async fn main() {
    env_logger::init();
    ygopro::init();
    let args = std::env::args().skip(1).collect::<Vec<String>>();
    let server_arguments = match parse_cli_args(&args, Default::default()) {
        Ok(parsed) => parsed,
        Err(error) => {
            log::error!("{error}");
            std::process::exit(1);
        }
    };
    let duel = build_duel_host(server_arguments.host_info, server_arguments.replay_mode, server_arguments.seeds);
    start_local_server(server_arguments.port, duel).await;
}
