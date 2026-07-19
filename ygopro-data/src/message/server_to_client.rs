#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]

use binrw::BinRead;
use binrw::BinWrite;
use ygopro_derive::Message;

use crate::generate_enum;
use crate::constants::CorePlayer;
use crate::constants::Netplayer;
use crate::constants::PlayerChange;
use crate::message::game_message;
use crate::utils::string::FixedLengthString;
use crate::utils::string::U16String;


use super::HostInfo;

include!(concat!(env!("OUT_DIR"), "/server_to_client.rs"));
every_server_to_client_flat_message!(generate_enum);

#[derive(BinRead, BinWrite, Debug, Clone, Message)]
#[message(stoc, flag = 1)]
pub struct GameMessage {
    pub message: game_message::Message
}

#[derive(BinRead, BinWrite, Debug, Clone, Message)]
#[message(stoc, flag = 2)]
#[repr(C)]
pub struct ErrorMessage {
    #[brw(pad_after = 3)]
    pub msg: u8,
    pub code: u32
}

#[derive(BinRead, BinWrite, Debug, Clone, Message)]
#[message(stoc, flag = 3)]
#[repr(C)]
pub struct SelectHand;

#[derive(BinRead, BinWrite, Debug, Clone, Message)]
#[message(stoc, flag = 4)]
#[repr(C)]
pub struct SelectTp;

#[derive(BinRead, BinWrite, Debug, Clone, Message)]
#[message(stoc, flag = 5)]
#[repr(C)]
pub struct HandResult {
    pub res1: crate::constants::Hand,
    pub res2: crate::constants::Hand
}

/// reserved, never sent by server
#[derive(BinRead, BinWrite, Debug, Clone, Message)]
#[message(stoc, flag = 6)]
#[repr(C)]
pub struct TpResult {
    pub result: u8
}

#[derive(BinRead, BinWrite, Debug, Clone, Message)]
#[message(stoc, flag = 7)]
#[repr(C)]
pub struct ChangeSide;

#[derive(BinRead, BinWrite, Debug, Clone, Message)]
#[message(stoc, flag = 8)]
#[repr(C)]
pub struct WaitingSide;

#[derive(BinRead, BinWrite, Debug, Clone, Message)]
#[message(stoc, flag = 9)]
#[repr(C)]
pub struct DeckCount {
    pub mainc_s: u16,
    pub sidec_s: u16,
    pub extrac_s: u16,
    pub mainc_o: u16,
    pub sidec_o: u16,
    pub extrac_o: u16
}

#[derive(BinRead, BinWrite, Debug, Clone, Message)]
#[message(stoc, flag = 17)]
#[repr(C)]
pub struct CreateGame {
    pub gameid: u32
}

#[derive(BinRead, BinWrite, Debug, Clone, Message)]
#[message(stoc, flag = 18)]
#[repr(C)]
pub struct JoinGame {
    pub info: HostInfo
}

#[derive(BinRead, BinWrite, Debug, Clone, Message)]
#[message(stoc, flag = 19)]
#[repr(C)]
pub struct TypeChange {
    pub _type: crate::constants::TypeChange
}
 
#[derive(BinRead, BinWrite, Debug, Clone, Message)]
#[message(stoc, flag = 20)]
#[repr(C)]
pub struct LeaveGame {
    pub pos: Netplayer
}

#[derive(BinRead, BinWrite, Debug, Clone, Message)]
#[message(stoc, flag = 21)]
#[repr(C)]
pub struct DuelStart;

#[derive(BinRead, BinWrite, Debug, Clone, Message)]
#[message(stoc, flag = 22)]
#[repr(C)]
pub struct DuelEnd;

// Rust enum use its variant's max size as its size.
// As we decide to use clone for message dispatching,
// The replay size is too large to be put in the enum, so we box it.
#[derive(BinRead, BinWrite, Debug, Clone, Message)]
#[message(stoc, flag = 23)]
pub struct Replay {
    pub replay: Box<crate::data::Replay>
}

#[derive(BinRead, BinWrite, Debug, Clone, Message)]
#[message(stoc, flag = 24)]
#[repr(C)]
pub struct TimeLimit {
    #[brw(pad_after = 1)]
    pub player: Netplayer,
    pub left_time: u16
}

#[derive(BinRead, BinWrite, Debug, Clone, Message)]
#[message(stoc, flag = 25)]
#[repr(C)]
pub struct Chat {
    #[br(map=|v: u16| CorePlayer::try_from(v as u8).unwrap_or(CorePlayer::None))]
    #[bw(map=|v: &CorePlayer| *v as u16)]
    pub player: CorePlayer,
    pub msg: U16String
}

#[derive(BinRead, BinWrite, Debug, Clone, Message)]
#[message(stoc, flag = 32)]
#[repr(C)]
pub struct HsPlayerEnter {
    pub name: FixedLengthString<20>,
    #[brw(pad_after = 1)]
    pub pos: Netplayer
}

#[derive(BinRead, BinWrite, Debug, Clone, Message)]
#[message(stoc, flag = 33)]
#[repr(C)]
pub struct HsPlayerChange {
    pub status: PlayerChange
}

#[derive(BinRead, BinWrite, Debug, Clone, Message)]
#[message(stoc, flag = 34)]
#[repr(C)]
pub struct HsWatchChange {
    pub watch_count: u16
}

#[derive(BinRead, BinWrite, Debug, Clone, Message)]
#[message(stoc, flag = 48)]
#[repr(C)]
pub struct FieldFinish;

#[cfg(test)]
mod test {
    #[test]
    fn print_sizes() {
        macro_rules! print_size {
            ($($msg:ident = $flag:literal),* $(,)?) => {
                println!("=== STOC ===");
                $(
                    println!("  {:30}: {:>4} bytes", stringify!($msg), std::mem::size_of::<super::$msg>());
                )*
                println!("  {:30}: {:>4} bytes", "MessageType", std::mem::size_of::<super::MessageType>());
                println!("  {:30}: {:>4} bytes", "Message", std::mem::size_of::<super::Message>());
            };
        }
        every_server_to_client_flat_message!(print_size);
    }
}
