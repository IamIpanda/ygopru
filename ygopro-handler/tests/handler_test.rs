use std::net::SocketAddr;

use ygopro_data::message::ctos;
use ygopro_handler::FrozenHandler;
use ygopro_handler::extract::Request;
use ygopro_handler::extract::Response;

fn handle_join_game(_addr: SocketAddr, _join: &ctos::JoinGame) {}

#[test]
fn join_game_is_valid_handler() {
    let _ = FrozenHandler::<Request<ctos::Message>, Response<ctos::Message>>::new(
        0,
        "test",
        "test",
        handle_join_game,
    );
}
