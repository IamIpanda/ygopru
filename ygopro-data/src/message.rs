//! The ygopro wire messages.
//!
//! This module holds the message types exchanged over the network and with the core:
//! client-to-server (`ctos`), server-to-client (`stoc`), and ygocore game messages (`gm`),
//! plus the flat `all` message type. It is the source of the `every_*_flat_message!`
//! macros.

pub mod client_to_server;
pub mod server_to_client;
pub mod game_message;
pub mod all_in_one;
mod utils;

pub use client_to_server as ctos;
pub use server_to_client as stoc;
pub use game_message as gm;
pub use all_in_one as all;
pub use utils::*;

