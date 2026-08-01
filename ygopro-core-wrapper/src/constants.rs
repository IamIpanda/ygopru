//! Mirrors of the buffer-length `#define` constants from `ocgcore/common.h`,
//! `ocgapi.h`, and the server framework `gframe/game.h`.

pub const SIZE_MESSAGE_BUFFER: usize = 0x2000;
pub const SIZE_RETURN_VALUE: usize = 512;
pub const SIZE_AI_NAME: usize = 128;
pub const SIZE_HINT_MSG: usize = 1024;
pub const SIZE_QUERY_BUFFER: usize = 0x40000;
