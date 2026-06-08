// 对应: ../ygopro/gframe/config.h + network.h + deck_manager.h + game.h

pub const PRO_VERSION: u16 = 0x1362;
pub const MAX_DATA_SIZE: usize = u16::MAX as usize - 1;
pub const SIZE_QUERY_BUFFER_SERVER: usize = 0x40000;
pub const MAX_MATCH_COUNT: usize = 3;
pub const LEN_CHAT_MSG: usize = 256;
pub const REPLAY_ID_YRP2: u32 = 0x32707279;
pub const REPLAY_MODE_INCLUDE_CHAT: u8 = 0x4;
pub const MAINC_MAX: usize = 250;
pub const SIDEC_MAX: usize = 250;
