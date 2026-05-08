#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]

use binrw::BinRead;
use binrw::BinWrite;
use ygopro_derive::Message;

use crate::generate_enum;
use crate::constants::Netplayer;
use crate::constants::PlayerChange;
use crate::message::game_message;
use crate::utils::string::FixedLengthString;
use crate::utils::string::U16String;


use super::HostInfo;

include!(concat!(env!("OUT_DIR"), "/server_to_client.rs"));
every_message!(generate_enum);

#[derive(BinRead, BinWrite, Debug, Message)]
#[message(stoc, flag = 1)]
pub struct GameMessage {
    message: game_message::Message
}

#[derive(BinRead, BinWrite, Debug, Message)]
#[message(stoc, flag = 2)]
#[repr(C)]
pub struct ErrorMessage {
    #[brw(pad_after = 3)]
    pub msg: crate::constants::ErrorMessage,
    pub code: u32
}

#[derive(BinRead, BinWrite, Debug, Message)]
#[message(stoc, flag = 3)]
#[repr(C)]
pub struct SelectHand;

#[derive(BinRead, BinWrite, Debug, Message)]
#[message(stoc, flag = 4)]
#[repr(C)]
pub struct SelectTp;

#[derive(BinRead, BinWrite, Debug, Message)]
#[message(stoc, flag = 5)]
#[repr(C)]
pub struct HandResult {
    pub res1: crate::constants::Hand,
    pub res2: crate::constants::Hand
}

#[derive(BinRead, BinWrite, Debug, Message)]
#[message(stoc, flag = 6)]
#[repr(C)]
pub struct TpResult {
    pub result: u8
}

#[derive(BinRead, BinWrite, Debug, Message)]
#[message(stoc, flag = 7)]
#[repr(C)]
pub struct ChangeSide;

#[derive(BinRead, BinWrite, Debug, Message)]
#[message(stoc, flag = 8)]
#[repr(C)]
pub struct WaitingSide;

#[derive(BinRead, BinWrite, Debug, Message)]
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

#[derive(BinRead, BinWrite, Debug, Message)]
#[message(stoc, flag = 17)]
#[repr(C)]
pub struct CreateGame {
    pub gameid: u32
}

#[derive(BinRead, BinWrite, Debug, Message)]
#[message(stoc, flag = 18)]
#[repr(C)]
pub struct JoinGame {
    pub info: HostInfo
}

#[derive(BinRead, BinWrite, Debug, Message)]
#[message(stoc, flag = 19)]
#[repr(C)]
pub struct TypeChange {
    pub _type: u8
}
 
#[derive(BinRead, BinWrite, Debug, Message)]
#[message(stoc, flag = 20)]
#[repr(C)]
pub struct LeaveGame {
    pub pos: Netplayer
}

#[derive(BinRead, BinWrite, Debug, Message)]
#[message(stoc, flag = 21)]
#[repr(C)]
pub struct DuelStart;

#[derive(BinRead, BinWrite, Debug, Message)]
#[message(stoc, flag = 22)]
#[repr(C)]
pub struct DuelEnd;

#[derive(BinRead, BinWrite, Debug, Message)]
#[message(stoc, flag = 23)]
#[repr(C)]
pub struct Replay {
    pub replay: crate::data::Replay
}

#[derive(BinRead, BinWrite, Debug, Message)]
#[message(stoc, flag = 24)]
#[repr(C)]
pub struct TimeLimit {
    #[brw(pad_after = 1)]
    pub player: Netplayer,
    pub left_time: u16
}

#[derive(BinRead, BinWrite, Debug, Message)]
#[message(stoc, flag = 25)]
#[repr(C)]
pub struct Chat {
    pub name: u16,
    pub msg: U16String
}

#[derive(BinRead, BinWrite, Debug, Message)]
#[message(stoc, flag = 32)]
#[repr(C)]
pub struct HsPlayerEnter {
    pub name: FixedLengthString<20>,
    #[brw(pad_after = 1)]
    pub pos: Netplayer
}

#[derive(BinRead, BinWrite, Debug, Message)]
#[message(stoc, flag = 33)]
#[repr(C)]
pub struct HsPlayerChange {
    pub status: PlayerChange
}

#[derive(BinRead, BinWrite, Debug, Message)]
#[message(stoc, flag = 34)]
#[repr(C)]
pub struct HsWatchChange {
    pub match_count: u16
}

#[derive(BinRead, BinWrite, Debug, Message)]
#[message(stoc, flag = 48)]
#[repr(C)]
pub struct FieldFinish;
