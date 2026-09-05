//! The room abstraction: turns a client-to-server stream into a server-to-client stream.
//!
//! A [`RoomProvider`] is a duel that accepts a client-to-server message stream and returns
//! the matching server-to-client message stream, plus a future that signals when the room
//! finishes.

use std::future::Future;

use futures::Stream;

/// Abstract of a duel.
/// 
/// Implementing this trait means it can accept a stream of `ClientToServerMessage` and
/// return a stream of `ServerToClientMessage`.
/// 
/// Usually `ClientToServerMessage` is a derived type of [`ctos::Message`](ygopro_data::message::ctos::Message), and
/// `ServerToClientMessage` is a derived type of [`stoc::Message`](ygopro_data::message::stoc::Message).
pub trait RoomProvider<ClientToServerMessage, ServerToClientMessage> {
    /// The stream of messages sent back to the client.
    type ServerToClientStream: Stream<Item = ServerToClientMessage> + Unpin + Send + 'static;
    /// A future that resolves when the room finishes.
    type FinishFuture: Future<Output = ()> + Unpin + Send + 'static;

    /// Add a client-to-server stream and return the corresponding server-to-client stream.
    fn add(&mut self, client_to_server_stream: impl Stream<Item = ClientToServerMessage> + Unpin + Send + 'static) -> Self::ServerToClientStream;
    /// Get a future that resolves when the room finishes.
    fn get_finish_signal(&mut self) -> Self::FinishFuture;
}
