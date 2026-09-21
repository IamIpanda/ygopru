//! Message traits and shared types.
//!
//! Provides the [`PureMessage`] / [`Message`] traits, the [`HostInfo`] struct, and the
//! message [`Error`] type.

use std::fmt::Debug;
use binrw::BinRead;
use binrw::BinWrite;

use crate::constants::MasterRule;
use crate::constants::Rule;

pub trait PureMessage: 'static {}

pub trait Message: PureMessage + Debug {
    fn message_type() -> crate::message::all::MessageType where Self: Sized;
}

#[derive(BinRead, BinWrite, Clone, Debug)]
#[repr(C)]
pub struct HostInfo {
    pub lflist: u32,
    pub rule: Rule,
    pub mode: crate::constants::Mode,
    pub duel_rule: crate::constants::MasterRule,
    #[br(map=|v:u8| v>0)]
    #[bw(map=|v| if *v {1u8} else {0u8})]
    pub no_check_deck: bool,
    #[br(map=|v:u8| v>0)]
    #[bw(map=|v| if *v {1u8} else {0u8})]
    #[brw(pad_after=3)]
    pub no_shuffle_deck: bool,
    pub start_lp: u32,
    pub start_hand: u8,
    pub draw_count: u8,
    pub time_limit: u16
}

impl Default for HostInfo {
    fn default() -> Self {
        Self { 
            lflist: 0, 
            rule: Rule::OCG,
            mode: crate::constants::Mode::Match, 
            duel_rule: MasterRule::MasterRule2020,
            no_check_deck: false, 
            no_shuffle_deck: false, 
            start_lp: 8000,
            start_hand: 5, 
            draw_count: 1, 
            time_limit: 180
        }
    }
}

#[cfg(feature = "forge")]
mod forge {
    use crate::complex::Complex;
    use crate::constants::MasterRule;
    use crate::constants::Netplayer;
    use crate::message::gm;
    use crate::message::stoc;
    use crate::utils::string::FixedLengthString;

    use super::HostInfo;

    const NO_TAG: &str = "---";

    /// Record the last duel of a recorded stoc stream into the ygopro-forge (aka ygopro2) records of a replay.
    ///
    /// The messages a replay cannot hold are dropped. The player names and the master
    /// rule met in the stream become the leading [`gm::SibylName`], and the messages
    /// recorded before the last duel start are left out.
    pub fn stoc_to_forge(recorded: &[Complex<stoc::Message>]) -> Vec<gm::Message> {
        let messages: Vec<&stoc::Message> = recorded.iter().filter_map(|message| message.try_get().ok()).collect();
        let start = messages.iter().rposition(|message| matches!(message, stoc::Message::DuelStart(_))).unwrap_or(0);
        let mut name = gm::SibylName {
            host_name: FixedLengthString::allocate(),
            host_tag_name: NO_TAG.into(),
            host_current_name: FixedLengthString::allocate(),
            client_name: FixedLengthString::allocate(),
            client_tag_name: NO_TAG.into(),
            client_current_name: FixedLengthString::allocate(),
            master_rule: MasterRule::MasterRule2020,
        };
        for message in &messages {
            match message {
                stoc::Message::JoinGame(join_game) => name.master_rule = join_game.info.duel_rule,
                stoc::Message::HsPlayerEnter(enter) => match enter.pos {
                    Netplayer::Player(0) => {
                        name.host_name = (&*enter.name).into();
                        name.host_current_name = (&*enter.name).into();
                    },
                    Netplayer::Player(1) => {
                        name.client_name = (&*enter.name).into();
                        name.client_current_name = (&*enter.name).into();
                    },
                    Netplayer::Player(2) => name.host_tag_name = (&*enter.name).into(),
                    Netplayer::Player(3) => name.client_tag_name = (&*enter.name).into(),
                    _ => ()
                },
                _ => ()
            }
        }
        let mut forge = vec![gm::Message::from(name.clone())];
        for message in &messages[start..] {
            match message {
                stoc::Message::GameMessage(game_message) => forge.push(game_message.message.clone().into()),
                stoc::Message::Chat(chat) => forge.push(gm::SibylChat::from((chat, &name)).into()),
                stoc::Message::Replay(replay) => forge.push(gm::SibylReplay { replay: replay.replay.clone() }.into()),
                _ => ()
            }
        }
        forge
    }

    /// Turn the ygopro forge(aka ygopro2) records of a replay back into the stoc stream a client plays.
    ///
    /// The leading [`gm::SibylName`] becomes the join response, the player entrances
    /// and the duel start a client needs before the recorded messages.
    pub fn forge_to_stoc(messages: &[gm::Message]) -> Vec<stoc::Message> {
        let mut stoc_messages = Vec::new();
        let name = messages.iter().find_map(|message| match message { gm::Message::SibylName(name) => Some(name), _ => None });
        for message in messages {
            match message {
                gm::Message::SibylName(name) => {
                    stoc_messages.push(stoc::Message::from(stoc::JoinGame { info: HostInfo { duel_rule: name.master_rule, ..Default::default() } }));
                    stoc_messages.push(stoc::Message::from(stoc::HsPlayerEnter { name: (&*name.host_name).into(), pos: Netplayer::Player(0) }));
                    stoc_messages.push(stoc::Message::from(stoc::HsPlayerEnter { name: (&*name.client_name).into(), pos: Netplayer::Player(1) }));
                    if has_tag(&name.host_tag_name) {
                        stoc_messages.push(stoc::Message::from(stoc::HsPlayerEnter { name: (&*name.host_tag_name).into(), pos: Netplayer::Player(2) }));
                    }
                    if has_tag(&name.client_tag_name) {
                        stoc_messages.push(stoc::Message::from(stoc::HsPlayerEnter { name: (&*name.client_tag_name).into(), pos: Netplayer::Player(3) }));
                    }
                    stoc_messages.push(stoc::Message::from(stoc::DuelStart));
                },
                gm::Message::SibylChat(chat) => stoc_messages.push(stoc::Chat::from((chat, name)).into()),
                gm::Message::SibylReplay(sibyl) => stoc_messages.push(stoc::Message::Replay(stoc::Replay { replay: sibyl.replay.clone() })),
                message => stoc_messages.push(stoc::Message::from(message.clone()))
            }
        }
        stoc_messages
    }

    fn has_tag(name: &FixedLengthString<50>) -> bool {
        !name.is_empty() && &**name != NO_TAG
    }
}

#[cfg(feature = "forge")]
pub use forge::forge_to_stoc;
#[cfg(feature = "forge")]
pub use forge::stoc_to_forge;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Something wrong when io")]
    IO(std::io::Error),
    #[error("Custom error")]
    Custom(String),
    #[error("Try to serialize a component over its design size")]
    Oversize,
    #[error("Deserialize finished, but remain some bytes")]
    Remain(Vec<u8>),
    #[error("Try to deserialize a seq without limit.")]
    Unlimited,
    #[error("Some error happened when unwrap the writer.")]
    UnwrapWriter,
    #[error("Try to deserialize to a wrong type.")]
    WrongType,
    #[error("Try to deserialize an unknown type message.")]
    UnknownType,
    #[error("Try to change full message to wrong status.")]
    WrongStatus,
}

#[macro_export]
macro_rules! generate_enum {
    ($($message_name:ident=$message_flag:literal),*) => {
        #[derive(binrw::BinRead, binrw::BinWrite, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug, Hash)]
        #[brw(repr=u8)]
        #[repr(u8)]
        pub enum MessageType {
            Unknown(u8),
            $($message_name=$message_flag),*
        }
        
        impl std::convert::From<u8> for MessageType {
            fn from(value: u8) -> Self {
                match value {
                    $($message_flag => Self::$message_name,)*
                    _ => Self::Unknown(value)
                }
            }
        }

        impl std::convert::From<MessageType> for u8 {
            fn from(value: MessageType) -> Self {
                match value {
                    $(MessageType::$message_name => $message_flag,)*
                    MessageType::Unknown(v) => v
                }
            }
        }

        impl std::convert::From<&MessageType> for u8 {
            fn from(value: &MessageType) -> Self {
                match *value {
                    $(MessageType::$message_name => $message_flag,)*
                    MessageType::Unknown(v) => v
                }
            }
        }

        #[derive(binrw::BinRead, binrw::BinWrite, Clone, Debug)]
        pub enum Message {
            $(#[brw(magic($message_flag))]
            $message_name($message_name)),*
        }

        impl crate::message::PureMessage for Message {}

        impl From<&Message> for MessageType {
            fn from(value: &Message) -> Self {
                match value {
                    $(Message::$message_name(_) => MessageType::$message_name),*
                }
            }
        }

        $(
            impl TryFrom<Message> for $message_name {
                type Error = crate::message::Error;

                fn try_from(value: Message) -> Result<Self, Self::Error> {
                    match value {
                        Message::$message_name(v) => Ok(v),
                        _ => Err(crate::message::Error::WrongType)
                    }
                }
            }

            impl<'m> TryFrom<&'m Message> for &'m $message_name {
                type Error = crate::message::Error;

                fn try_from(value: &'m Message) -> Result<Self, Self::Error> {
                    match value {
                        Message::$message_name(v) => Ok(v),
                        _ => Err(crate::message::Error::WrongType)
                    }
                }
            }
            
            impl From<$message_name> for Message {
                fn from(value: $message_name) -> Self {
                    Message::$message_name(value)
                }
            }

            impl $message_name {
                pub fn into_message(self) -> Message { 
                    self.into() 
                } 
            }
        )*
    };
}

mod test {
    #![allow(unused_imports)]

    use std::io::Cursor;
    use binrw::BinRead;
    use binrw::BinWrite;

    use crate::message::client_to_server::HandResult;
    use crate::message::client_to_server::JoinGame;
    use crate::message::client_to_server::MessageType;
    use crate::message::client_to_server::Message;
    
    #[test]
    fn test_message_type_basic() {
        let message_type = MessageType::CreateGame;
        let mut vec = Cursor::new(Vec::<u8>::new());
        message_type.write_le(&mut vec).unwrap();
        assert_eq!(vec.into_inner(), [17]);

        let mut vec = Cursor::new(vec![127]);
        let message_type = MessageType::read_le(&mut vec).unwrap();
        assert_eq!(message_type, MessageType::Unknown(127));
    }

    #[test]
    fn test_message_enum_basic() {
        let message_enum = Message::HandResult(HandResult {
            res: crate::constants::Hand::Paper
        });
        let mut vec = Cursor::new(Vec::<u8>::new());
        message_enum.write_le(&mut vec).unwrap(); 
        println!("{:?}", vec.into_inner());
    }
}
