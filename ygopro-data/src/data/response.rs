use binrw::BinRead;
use binrw::BinWrite;
use binrw::VecArgs;
use binrw::helpers::until_eof;
use binrw::io::Cursor;
use binrw::io::Read;
use binrw::io::Seek;
use binrw::io::Write;

use crate::constants::*;
use crate::message::gm;

#[derive(BinRead, BinWrite, Debug, Clone, Copy, PartialEq, Eq)]
#[brw(repr = u16)]
#[repr(u16)]
pub enum IdleCommand {
    Summon = 0,
    SpecialSummon = 1,
    Reposition = 2,
    SetMonster = 3,
    SetSpellTrap = 4,
    Activate = 5,
    EnterBattlePhase = 6,
    EnterEndPhase = 7,
    ShuffleDeck = 8,
}

#[derive(BinRead, BinWrite, Debug, Clone, Copy, PartialEq, Eq)]
#[brw(repr = u16)]
#[repr(u16)]
pub enum BattleCommand {
    Activate = 0,
    Attack = 1,
    EnterMainPhase2 = 2,
    EnterEndPhase = 3,
}

#[derive(Debug, Clone)]
pub enum Response {
    Cancel,
    SelectIdleCommand(IdleCommand, u16),
    SelectBattleCommand(BattleCommand, u16),
    SelectYesNo(bool),
    SelectOption(u8),
    SelectChain(u8),
    DeclineChain,
    SelectPosition(Position),
    SelectCards(Vec<u16>),
    SelectUnselectCards(u16),
    SelectTribute(Vec<u16>),
    SelectSum(Vec<u16>),
    SelectCounter(Vec<u16>),
    SelectPlace(CorePlayer, Location, u8),
    DeclinePlace,
    SortCards(Vec<u16>),
    KeepCardOrder,
    AnnounceRace(Race),
    AnnounceAttribute(Attribute),
    AnnounceCard(u32),
    Unknown(Vec<u8>),
}

impl BinWrite for Response {
    type Args<'a> = ();

    fn write_options<W: Write + Seek>(&self, writer: &mut W, endian: binrw::Endian, _args: ()) -> binrw::BinResult<()> {
        let args = ();
        match self {
            Response::Cancel => (-1i32).write_options(writer, endian, args)?,
            Response::SelectIdleCommand(command, card_index) => {
                command.write_options(writer, endian, args)?;
                card_index.write_options(writer, endian, args)?;
            }
            Response::SelectBattleCommand(command, card_index) => {
                command.write_options(writer, endian, args)?;
                card_index.write_options(writer, endian, args)?;
            }
            Response::SelectYesNo(yes) => u8::from(*yes).write_options(writer, endian, args)?,
            Response::SelectOption(index) => index.write_options(writer, endian, args)?,
            Response::SelectChain(index) => index.write_options(writer, endian, args)?,
            Response::DeclineChain => (-1i32).write_options(writer, endian, args)?,
            Response::SelectPosition(position) => position.write_options(writer, endian, args)?,
            Response::SelectCards(card_indices) => {
                let len = card_indices.len() as u8;
                len.write_options(writer, endian, args)?;
                card_indices.write_options(writer, endian, args)?;
            }
            Response::SelectUnselectCards(card_index) => {
                1u8.write_options(writer, endian, args)?;
                card_index.write_options(writer, endian, args)?;
            }
            Response::SelectTribute(card_indices) => {
                let len = card_indices.len() as u8;
                len.write_options(writer, endian, args)?;
                card_indices.write_options(writer, endian, args)?;
            }
            Response::SelectSum(card_indices) => {
                let len = card_indices.len() as u8;
                len.write_options(writer, endian, args)?;
                card_indices.write_options(writer, endian, args)?;
            }
            Response::SelectCounter(counts) => counts.write_options(writer, endian, args)?,
            Response::SelectPlace(player, location, sequence) => {
                player.write_options(writer, endian, args)?;
                location.write_options(writer, endian, args)?;
                sequence.write_options(writer, endian, args)?;
            }
            Response::DeclinePlace => [0u8; 3].write_options(writer, endian, args)?,
            Response::SortCards(order) => order.write_options(writer, endian, args)?,
            Response::KeepCardOrder => 0xffu8.write_options(writer, endian, args)?,
            Response::AnnounceRace(races) => races.write_options(writer, endian, args)?,
            Response::AnnounceAttribute(attributes) => attributes.write_options(writer, endian, args)?,
            Response::AnnounceCard(code) => code.write_options(writer, endian, args)?,
            Response::Unknown(data) => data.write_options(writer, endian, args)?,
        }
        Ok(())
    }
}

impl Response {
    pub fn len(&self) -> usize {
        match self {
            Response::Cancel => 4,
            Response::SelectIdleCommand(_, _) => 4,
            Response::SelectBattleCommand(_, _) => 4,
            Response::SelectYesNo(_) => 1,
            Response::SelectOption(_) => 1,
            Response::SelectChain(_) => 1,
            Response::DeclineChain => 4,
            Response::SelectPosition(_) => 1,
            Response::SelectCards(card_indices) => 1 + card_indices.len() * 2,
            Response::SelectUnselectCards(_) => 3,
            Response::SelectTribute(card_indices) => 1 + card_indices.len() * 2,
            Response::SelectSum(card_indices) => 1 + card_indices.len() * 2,
            Response::SelectCounter(counts) => counts.len() * 2,
            Response::SelectPlace(_, _, _) => 3,
            Response::DeclinePlace => 3,
            Response::SortCards(order) => order.len() * 2,
            Response::KeepCardOrder => 1,
            Response::AnnounceRace(_) => 4,
            Response::AnnounceAttribute(_) => 4,
            Response::AnnounceCard(_) => 4,
            Response::Unknown(data) => data.len(),
        }
    }

    pub fn resolve(&mut self, message_type: gm::MessageType) -> binrw::BinResult<()> {
        let data = match self {
            Response::Unknown(data) => data,
            _ => return Ok(()),
        };
        *self = Self::parse(&mut Cursor::new(data.as_slice()), binrw::Endian::Little, message_type)?;
        Ok(())
    }

    fn parse<R: Read + Seek>(reader: &mut R, endian: binrw::Endian, message_type: gm::MessageType) -> binrw::BinResult<Response> {
        match message_type {
            gm::MessageType::SelectIdleCommand => Ok(Response::SelectIdleCommand(IdleCommand::read_options(reader, endian, ())?, u16::read_options(reader, endian, ())?)),
            gm::MessageType::SelectBattleCommand => Ok(Response::SelectBattleCommand(BattleCommand::read_options(reader, endian, ())?, u16::read_options(reader, endian, ())?)),
            gm::MessageType::SelectEffectYesNo | gm::MessageType::SelectYesNo => Ok(Response::SelectYesNo(u8::read_options(reader, endian, ())? != 0)),
            gm::MessageType::SelectOption | gm::MessageType::AnnounceNumber => Ok(Response::SelectOption(u8::read_options(reader, endian, ())?)),
            gm::MessageType::SelectPosition => Ok(Response::SelectPosition(Position::read_options(reader, endian, ())?)),
            gm::MessageType::SelectCounter => Ok(Response::SelectCounter(until_eof::<_, u16, (), Vec<u16>>(reader, endian, ())?)),
            gm::MessageType::AnnounceRace => Ok(Response::AnnounceRace(Race::read_options(reader, endian, ())?)),
            gm::MessageType::AnnounceAttribute => Ok(Response::AnnounceAttribute(Attribute::read_options(reader, endian, ())?)),
            gm::MessageType::AnnounceCard => Ok(Response::AnnounceCard(u32::read_options(reader, endian, ())?)),
            gm::MessageType::SelectChain => {
                if read_all_remaining(reader)? == &(-1i32).to_le_bytes() {
                    return Ok(Response::DeclineChain);
                }
                Ok(Response::SelectChain(u8::read_options(reader, endian, ())?))
            }
            gm::MessageType::SelectCard => {
                if read_all_remaining(reader)? == &(-1i32).to_le_bytes() {
                    return Ok(Response::Cancel);
                }
                let len = u8::read_options(reader, endian, ())?;
                Ok(Response::SelectCards(Vec::<u16>::read_options(reader, endian, VecArgs { count: len as usize, inner: () })?))
            }
            gm::MessageType::SelectUnselectCard => {
                if read_all_remaining(reader)? == &(-1i32).to_le_bytes() {
                    return Ok(Response::Cancel);
                }
                let marker = u8::read_options(reader, endian, ())?;
                if marker != 1 {
                    return Err(binrw::Error::NoVariantMatch { pos: reader.stream_position()? });
                }
                Ok(Response::SelectUnselectCards(u16::read_options(reader, endian, ())?))
            }
            gm::MessageType::SelectTribute => {
                if read_all_remaining(reader)? == &(-1i32).to_le_bytes() {
                    return Ok(Response::Cancel);
                }
                let len = u8::read_options(reader, endian, ())?;
                Ok(Response::SelectTribute(Vec::<u16>::read_options(reader, endian, VecArgs { count: len as usize, inner: () })?))
            }
            gm::MessageType::SelectSum => {
                let len = u8::read_options(reader, endian, ())?;
                Ok(Response::SelectSum(Vec::<u16>::read_options(reader, endian, VecArgs { count: len as usize, inner: () })?))
            }
            gm::MessageType::SelectPlace | gm::MessageType::SelectDisableField => {
                let player = CorePlayer::read_options(reader, endian, ())?;
                let location = Location::read_options(reader, endian, ())?;
                let sequence = u8::read_options(reader, endian, ())?;
                if player == CorePlayer::FirstAttackPlayer && location.is_empty() && sequence == 0 {
                    Ok(Response::DeclinePlace)
                } else {
                    Ok(Response::SelectPlace(player, location, sequence))
                }
            }
            gm::MessageType::SortCard => {
                if read_all_remaining(reader)? == &[0xffu8] {
                    return Ok(Response::KeepCardOrder);
                }
                Ok(Response::SortCards(until_eof::<_, u16, (), Vec<u16>>(reader, endian, ())?))
            }
            _ => Err(binrw::Error::NoVariantMatch { pos: reader.stream_position()? }),
        }
    }
}

impl BinRead for Response {
    type Args<'a> = Option<gm::MessageType>;

    fn read_options<R: Read + Seek>(reader: &mut R, endian: binrw::Endian, message_type: Self::Args<'_>) -> binrw::BinResult<Self> {
        match message_type {
            Some(message_type) => Self::parse(reader, endian, message_type),
            None => Ok(Response::Unknown(until_eof::<_, u8, (), Vec<u8>>(reader, endian, ())?)),
        }
    }
}

fn read_all_remaining<R: Read + Seek>(reader: &mut R) -> binrw::BinResult<Vec<u8>> {
    let pos = reader.stream_position()?;
    let mut data = Vec::new();
    reader.read_to_end(&mut data)?;
    reader.seek(std::io::SeekFrom::Start(pos))?;
    Ok(data)
}
