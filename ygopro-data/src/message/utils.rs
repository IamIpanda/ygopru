use std::fmt::Debug;
use std::io::Cursor;
use binrw::BinRead;
use binrw::BinWrite;

pub trait PureMessage: 'static {}

pub trait Message: PureMessage + Debug {
    fn message_type() -> crate::message::all::MessageType where Self: Sized;
}

#[derive(BinRead, BinWrite, Clone, Debug)]
#[repr(C)]
pub struct HostInfo {
    pub lflist: i32,
    pub rule: u8,
    pub mode: crate::constants::Mode,
    pub duel_rule: u8,
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
            rule: 0, 
            mode: crate::constants::Mode::Match, 
            duel_rule: 5,
            no_check_deck: false, 
            no_shuffle_deck: false, 
            start_lp: 8000,
            start_hand: 5, 
            draw_count: 1, 
            time_limit: 180
        }
    }
}

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

        #[derive(binrw::BinRead, binrw::BinWrite, Debug)]
        pub enum Message {
            $(#[brw(magic($message_flag))]
            $message_name($message_name)),*
        }

        impl crate::message::PureMessage for Message {}

        pub type MessageComplex<Source> = crate::message::MessageComplex<Source, MessageType, Message>;

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

pub fn is_data_full(data: &[u8]) -> bool {
    let mut cursor = Cursor::new(data);
    let size = data.len();
    let mut pos = 0usize;
    loop {
        let len = match u16::read_le(&mut cursor) {
            Ok(len) => len,
            Err(_) => return false
        };
        pos += 2usize + len as usize;
        if pos > size { return false }
        else if pos == size { return true }
        else { cursor.set_position(pos as u64); }
    }
}

mod length {
    use std::io::Cursor;
    use std::ops::Deref;

    use binrw::BinRead;
    use binrw::BinWrite;

    #[derive(Debug)]
    pub struct LengthWrapper<T> (pub T);
    impl<T: BinRead> BinRead for LengthWrapper<T> {
        type Args<'a> = <T as BinRead>::Args<'a>;

        fn read_options<R: std::io::prelude::Read + std::io::prelude::Seek>(reader: &mut R, endian: binrw::Endian, args: Self::Args<'_>,) -> binrw::prelude::BinResult<Self> {
            u16::read_options(reader, endian, ())?;
            T::read_options(reader, endian, args).map(|v| LengthWrapper(v))
        }
    }

    impl <T: BinWrite> BinWrite for LengthWrapper<T> {
        type Args<'a> = <T as BinWrite>::Args<'a>;

        fn write_options<W: std::io::prelude::Write + std::io::prelude::Seek>(&self, writer: &mut W, endian: binrw::Endian, args: Self::Args<'_>,) -> binrw::prelude::BinResult<()> {
            let mut vec = Cursor::new(Vec::<u8>::new());
            self.0.write_options(&mut vec, endian, args)?;
            let vec = vec.into_inner();
            (vec.len() as u16).write_options(writer, endian, ())?;
            Ok(vec.write_options(writer, endian, ())?)
        }
    }

    impl<T> Deref for LengthWrapper<T> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

}

mod complex {
    use std::io::Cursor;
    use std::ops::Deref;

    use binrw::BinRead;
    use binrw::BinResult;
    use once_cell::sync::OnceCell;

    use super::length::LengthWrapper;

    pub trait MessageCell {
        type Source: AsRef<[u8]>;
        type Type: From<u8> + Copy;
        type Enum: BinRead;

        fn new(source: Self::Source) -> Self;
        fn from_slice(bytes: Self::Source, slicer: fn(Self::Source, usize, usize) -> Self::Source) -> (Vec<Self>, Option<Self::Source>) where Self: Sized;
        fn message_type(&self) -> Self::Type;
        fn message_enum(&self) -> BinResult<&Self::Enum>;
        fn into_inner(self) -> Self::Source;
    }

    #[derive(Debug)]
    pub struct MessageComplex<Source ,Type, Enum> {
        bytes: Source,
        message_type: OnceCell<Type>,
        message_enum: OnceCell<LengthWrapper<Enum>>
    }

    impl<Source, Type, Enum> MessageComplex<Source, Type, Enum> {
        pub fn new(bytes: Source) -> Self {
            Self { bytes, message_type: OnceCell::new(), message_enum: OnceCell::new() }
        }

        pub fn inner(&self) -> &Source {
            return &self.bytes;
        }
    }

    impl<Source, Type, Enum> MessageComplex<Source, Type, Enum> where Source: AsRef<[u8]> + Clone {
        pub fn from_bytes(bytes: &mut Cursor<Source>, slicer: fn(Source, usize, usize) -> Source) -> Option<Self> {
            let pos = bytes.position() as usize;
            let length = u16::read_le(bytes).ok()? as usize;
            let remain = bytes.get_ref().as_ref().len() - pos;
            if length > remain { return None; }
            bytes.set_position((pos + length + 2) as u64);
            Some(Self::new(slicer(bytes.get_ref().clone(), pos, pos+length+2)))
        }

        pub fn from_slice(bytes: Source, slicer: fn(Source, usize, usize) -> Source) -> (Vec<Self>, Option<Source>) {
            let mut messages: Vec<MessageComplex<Source, Type, Enum>> = Vec::new();
            let mut cursor = Cursor::new(bytes);
            while let Some(message) = Self::from_bytes(&mut cursor, slicer) {
                messages.push(message);
            }
            let pos = cursor.position() as usize;
            let bytes = cursor.into_inner();
            let len = bytes.as_ref().len();
            let rest = if pos == len { None } else { Some(slicer(bytes, pos, len)) };
            (messages, rest)
        }
    }

    impl<Source, Type, Enum> MessageComplex<Source, Type, Enum> where Source: AsRef<[u8]>, Type: From<u8> + Copy {
        pub fn message_type(&self) -> Type {
            let _type = self.bytes.as_ref()[2];
            return *self.message_type.get_or_init(|| _type.into());
        }
    }

    impl<Source, Type, Enum> MessageComplex<Source, Type, Enum> where 
        Source: AsRef<[u8]>,
        Enum: BinRead, 
        for<'a> <Enum as BinRead>::Args<'a>: Default 
    {
        pub fn message_enum(&self) -> BinResult<&Enum> {
            let wrapper = self.message_enum.get_or_try_init(|| {
                let mut cursor = Cursor::new(&self.bytes.as_ref()[0..]);
                LengthWrapper::<Enum>::read_le(&mut cursor)
            })?;
            Ok(wrapper.deref())
        }
    }

    impl<Source, Type, Enum> Deref for MessageComplex<Source, Type, Enum> where Source: AsRef<[u8]> {
        type Target = [u8];

        fn deref(&self) -> &Self::Target {
            &self.bytes.as_ref()
        }
    }

    impl<Source, Type, Enum> AsRef<[u8]> for MessageComplex<Source, Type, Enum> where Source: AsRef<[u8]> {
        fn as_ref(&self) -> &[u8] {
            self.bytes.as_ref()
        }
    }

    pub fn ref_slicer(source: &[u8], from: usize, to: usize) -> &[u8] {
        &source[from..to]
    }


    impl<Source, Type, Enum> MessageCell for MessageComplex<Source, Type, Enum> 
        where Source: AsRef<[u8]> + Clone,
              Type: From<u8> + Copy,
              Enum: BinRead,
              for<'a> <Enum as BinRead>::Args<'a>: Default
    {
        type Source = Source;
        type Type = Type;
        type Enum = Enum;
        fn new(source: Source) -> Self {
            Self::new(source)
        }
        fn from_slice(bytes: Source, slicer: fn(Source, usize, usize) -> Source) -> (Vec<Self>, Option<Source>) where Self: Sized {
            Self::from_slice(bytes, slicer)
        }

        fn message_type(&self) -> Self::Type {
            Self::message_type(&self)
        }

        fn message_enum(&self) -> BinResult<&Self::Enum> {
            Self::message_enum(&self)
        }
        
        fn into_inner(self) -> Source {
            self.bytes
        }
    }
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

#[allow(unused_imports)]
pub use complex::*;
#[allow(unused_imports)]
pub use length::*;
