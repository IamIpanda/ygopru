use std::net::SocketAddr;

use ygopro_data::message::ctos;
use ygopro_handler::handler::State;
use ygopro_handler::handler::async_handler::AsyncHandler;
use ygopro_handler::handler::sync_handler::SyncHandler;
use ygopro_handler::handler::tower_handler::TowerHandler;
use ygopro_handler::extract::Request;
use ygopro_handler::extract::Response;

fn handle_join_game(_addr: SocketAddr, _join: &ctos::JoinGame) {}

#[test]
fn tower_handler_accepts_sync_handler() {
    let _ = TowerHandler::<Request<ctos::Message>, State, Response<ctos::Message>>::new(
        0,
        "tower",
        "test",
        handle_join_game,
    );
}

#[test]
fn async_handler_accepts_sync_handler() {
    let _ = AsyncHandler::<Request<ctos::Message>, State, Response<ctos::Message>>::new(
        0,
        "async",
        "test",
        handle_join_game,
    );
}

#[test]
fn sync_handler_accepts_sync_handler() {
    let _ = SyncHandler::<Request<ctos::Message>, State, Response<ctos::Message>>::new(
        0,
        "sync",
        "test",
        handle_join_game,
    );
}
