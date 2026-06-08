#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]

use binrw::BinRead;
use binrw::BinWrite;
use num_enum::IntoPrimitive;
use num_enum::TryFromPrimitive;
use bitflags::bitflags;

#[derive(BinRead, BinWrite, Copy, Clone, Eq, PartialEq, TryFromPrimitive, IntoPrimitive, Debug)]
#[brw(repr=u16)]
#[repr(u16)]
pub enum Network {
    ServerId = 29736,
    ClientId = 57078,
}

#[derive(BinRead, BinWrite, Copy, Clone, Eq, PartialEq, TryFromPrimitive, IntoPrimitive, Debug, PartialOrd, Ord, Hash)]
#[brw(repr=u8)]
#[repr(u8)]
pub enum Netplayer {
    Player1 = 0,
    Player2 = 1,
    Player3 = 2,
    Player4 = 3,
    Player5 = 4,
    Player6 = 5,
    Observer = 7,
}

impl std::default::Default for Netplayer {
    fn default() -> Self {
        return Netplayer::Observer;
    }
}

#[derive(BinRead, BinWrite, Copy, Clone, Eq, PartialEq, TryFromPrimitive, IntoPrimitive, Debug, PartialOrd, Ord, Hash)]
#[brw(repr=u8)]
#[repr(u8)]
pub enum LocalPlayer {
    FirstPlayer = 0,
    SecondPlayer = 1,
    None = 2,
    All = 3,
    /// This value is only used as `reason_player` when reason is rule.
    Rule = 5,
}

// Great fukcing structure design need great adapter codes.
#[derive(BinRead, BinWrite, Copy, Clone, Eq, PartialEq, Debug)]
#[brw(repr=u8)]
pub enum PlayerChange {
    Enter(Netplayer),
    Observe(Netplayer),
    Ready(Netplayer),
    Notready(Netplayer),
    Leave(Netplayer),
}

impl PlayerChange {
    fn as_u8(&self) -> u8 {
        match *self {
            PlayerChange::Enter(player) => player as u8,
            PlayerChange::Observe(player) => player as u8 * 16 + 8,
            PlayerChange::Ready(player) => player as u8 * 16 + 9,
            PlayerChange::Notready(player) => player as u8 * 16 + 10,
            PlayerChange::Leave(player) => player as u8 * 16 + 11,
        }
    }
}

impl num_enum::TryFromPrimitive for PlayerChange {
    type Primitive = u8;
    const NAME: &'static str = "PlayerChange";
    fn try_from_primitive(source: Self::Primitive) -> Result<Self, Self::Error> {
        if source < 8 { return Netplayer::try_from_primitive(source).map_or_else(|_| Err(num_enum::TryFromPrimitiveError { number: source }), |t| Ok(PlayerChange::Enter(t))) }
        let position = (source & 0xf0) >> 4;
        let player = match Netplayer::try_from_primitive(position) {
            Ok(player) => player,
            Err(_) => return Err(num_enum::TryFromPrimitiveError { number: source })
        };
        let operation = source & 0xf;
        match operation {
            8 => Ok(PlayerChange::Observe(player)),
            9 => Ok(PlayerChange::Ready(player)),
            10 => Ok(PlayerChange::Notready(player)),
            11 => Ok(PlayerChange::Leave(player)),
            _ => Err(num_enum::TryFromPrimitiveError { number: source })
        }
    }

    type Error = num_enum::TryFromPrimitiveError<Self>;
}

impl std::convert::TryFrom<u8> for PlayerChange {
    type Error = num_enum::TryFromPrimitiveError<Self>;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::try_from_primitive(value)
    }
}

impl std::convert::From<&PlayerChange> for u8 {
    fn from(value: &PlayerChange) -> Self {
        value.as_u8()
    }
}

#[derive(BinRead, BinWrite, Copy, Clone, Eq, PartialEq, TryFromPrimitive, IntoPrimitive, Debug)]
#[brw(repr=u8)]
#[repr(u8)]
pub enum ErrorMessage {
    JoinError = 1,
    DeckError = 2,
    SideError = 3,
    VersionError = 4,
}

#[derive(BinRead, BinWrite, Copy, Clone, Eq, PartialEq, TryFromPrimitive, IntoPrimitive, Debug, Hash)]
#[brw(repr=u8)]
#[repr(u8)]
pub enum Mode {
    Single = 0,
    Match = 1,
    Tag = 2,
}

bitflags! {
    #[repr(transparent)]
    #[derive(BinRead, BinWrite, Copy, Clone, Eq, PartialEq, Debug)]
    #[br(map=|x| Self::from_bits_retain(x))]
    #[bw(map=|x: &Self| x.bits())]
    pub struct Location: u8 {
        const Limbo = 0;
        const Deck = 1;
        const Hand = 2;
        const MZone = 4;
        const SZone = 8;
        const Grave = 16;
        const Removed = 32;
        const Extra = 64;
        const Overlay = 128;
        // OnField = 12;
        // FZone = 256,
        // PZone = 512,
        // DeckBot = 65537,
        // DeckShf = 131073,
    }
}

#[derive(BinRead, BinWrite, Copy, Clone, Eq, PartialEq, TryFromPrimitive, IntoPrimitive, Debug)]
#[brw(repr=u8)]
#[repr(u8)]
pub enum Position {
    Any = 0,
    FaceupAttack = 1,
    FaceDownAttack = 2,
    FaceupDefense = 4,
    FacedownDefense = 8,
    Faceup = 5,
    Facedown = 10,
    Attack = 3,
    Defense = 12,
    // NoFlipEffect = 65536
}

bitflags! {
    #[repr(transparent)]
    #[derive(BinRead, BinWrite, Clone, Copy, Default, Debug)]
    #[br(map=|x| Self::from_bits_retain(x))]
    #[bw(map=|x: &Self| x.bits())]
    pub struct Timing: u32 {
        const DrawPhase = 1;
        const StandbyPhase = 2;
        const MainEnd = 4;
        const BattleStart = 8;
        const BattleEnd = 16;
        const EndPhase = 32;
        const Summon = 64;
        const Spsummon = 128;
        const Flipsummon = 256;
        const Mset = 512;
        const Sset = 1024;
        const PosChange = 2048;
        const Attack = 4096;
        const DamageStep = 8192;
        const DamageCal = 16384;
        const ChainEnd = 32768;
        const Draw = 65536;
        const Damage = 131072;
        const Recover = 262144;
        const Destroy = 524288;
        const Remove = 1048576;
        const Tohand = 2097152;
        const Todeck = 4194304;
        const Tograve = 8388608;
        const BattlePhase = 16777216;
        const Equip = 33554432;
    }
}

bitflags! {
    #[repr(transparent)]
    #[derive(BinRead, BinWrite, Clone, Copy, Default, Debug)]
    #[br(map=|x| Self::from_bits_retain(x))]
    #[bw(map=|x: &Self| x.bits())]
    pub struct Type: u32 {
        const Monster = 1;
        const Spell = 2;
        const Trap = 4;
        const Normal = 16;
        const Effect = 32;
        const Fusion = 64;
        const Ritual = 128;
        const Trapmonster = 256;
        const Spirit = 512;
        const Union = 1024;
        const Dual = 2048;
        const Tuner = 4096;
        const Synchro = 8192;
        const Token = 16384;
        const Quickplay = 65536;
        const Continuous = 131072;
        const Equip = 262144;
        const Field = 524288;
        const Counter = 1048576;
        const Flip = 2097152;
        const Toon = 4194304;
        const Xyz = 8388608;
        const Pendulum = 16777216;
        const Spsummon = 33554432;
        const Link = 67108864;
    }
}


bitflags! {
    #[repr(transparent)]
    #[derive(BinRead, BinWrite, Clone, Copy, Default, Debug)]
    #[br(map=|x| Self::from_bits_retain(x))]
    #[bw(map=|x: &Self| x.bits())]
    pub struct Race: u32 {
        const Warrior = 1;
        const Spellcaster = 2;
        const Fairy = 4;
        const Fiend = 8;
        const Zombie = 16;
        const Machine = 32;
        const Aqua = 64;
        const Pyro = 128;
        const Rock = 256;
        const Windbeast = 512;
        const Plant = 1024;
        const Insect = 2048;
        const Thunder = 4096;
        const Dragon = 8192;
        const Beast = 16384;
        const Beastwarrior = 32768;
        const Dinosaur = 65536;
        const Fish = 131072;
        const Seaserpent = 262144;
        const Reptile = 524288;
        const Psycho = 1048576;
        const Devine = 2097152;
        const Creatorgod = 4194304;
        const Wyrm = 8388608;
        const Cyberse = 16777216;
        const Illusion = 33554432;
    }
}


bitflags! {
    #[repr(transparent)]
    #[derive(BinRead, BinWrite, Clone, Copy, Default, Debug)]
    #[br(map=|x| Self::from_bits_retain(x))]
    #[bw(map=|x: &Self| x.bits())]
    pub struct Reason: u32 {
        const Destroy = 0x1;
        const Release = 0x2;
        const Temporary = 0x4;
        const Material = 0x8;
        const Summon = 0x10;
        const Battle = 0x20;
        const Effect = 0x40;
        const Cost = 0x80;
        const Adjust = 0x100;
        const LostTarget = 0x200;
        const Rule = 0x400;
        const Spsummon = 0x800;
        const Dissummon = 0x1000;
        const Flip = 0x2000;
        const Discard = 0x4000;
        const Rdamage = 0x8000;
        const Rrecover = 0x10000;
        const Return = 0x20000;
        const Fusion = 0x40000;
        const Synchro = 0x80000;
        const Ritual = 0x100000;
        const Xyz = 0x200000;
        const Replace = 0x1000000;
        const Draw = 0x2000000;
        const Redirect = 0x4000000;
        const Reveal = 0x8000000;
        const Link = 0x10000000;
        const LostOverlay = 0x20000000;
        const Maintenance = 0x40000000;
        const Action = 0x80000000;
    }
}

bitflags! {
    #[repr(transparent)]
    #[derive(BinRead, BinWrite, Clone, Copy, Default, Debug)]
    #[br(map=|x| Self::from_bits_retain(x))]
    #[bw(map=|x: &Self| x.bits())]
    pub struct Status: u32 {
        const Disabled = 0x0001;
        const ToEnable = 0x0002;
        const ToDisable = 0x0004;
        const ProcComplete = 0x0008;
        const SetTurn = 0x0010;
        const NoLevel = 0x0020;
        const BattleResult = 0x0040;
        const SpsummonStep = 0x0080;
        const CannotChangeForm = 0x0100;
        const Summoning = 0x0200;
        const EffectEnabled = 0x0400;
        const SummonTurn = 0x0800;
        const DestroyConfirmed = 0x1000;
        const LeaveConfirmed = 0x2000;
        const BattleDestroyed = 0x4000;
        const CopyingEffect = 0x8000;
        const Chaining = 0x10000;
        const SummonDisabled = 0x20000;
        const ActivateDisabled = 0x40000;
        const EffectReplaced = 0x80000;
        const FlipSummoning = 0x100000;
        const AttackCanceled = 0x200000;
        const Initializing = 0x400000;
        const ToHandWithoutConfirm = 0x800000;
        const JustPos = 0x1000000;
        const ContinuousPos = 0x2000000;
        const Forbidden = 0x4000000;
        const ActFromHand = 0x8000000;
        const OppoBattle = 0x10000000;
        const FlipSummonTurn = 0x20000000;
        const SpsummonTurn = 0x40000000;
        const FlipSummonDisabled = 0x80000000;
    }
}

bitflags! {
    #[repr(transparent)]
    #[derive(BinRead, BinWrite, Clone, Copy, Default, Debug)]
    #[br(map=|x| Self::from_bits_retain(x))]
    #[bw(map=|x: &Self| x.bits())]
    pub struct Query: u32 {
        const Code = 0x1;
        const Position = 0x2;
        const Alias = 0x4;
        const Type = 0x8;
        const Level = 0x10;
        const Rank = 0x20;
        const Attribute = 0x40;
        const Race = 0x80;
        const Attack = 0x100;
        const Defense = 0x200;
        const BaseAttack = 0x400;
        const BaseDefense = 0x800;
        const Reason = 0x1000;
        const ReasonCard = 0x2000;
        const EquipCard = 0x4000;
        const TargetCard = 0x8000;
        const OverlayCard = 0x10000;
        const Counters = 0x20000;
        const Owner = 0x40000;
        const Status = 0x80000;
        const LeftScale = 0x200000;
        const RightScale = 0x400000;
        const Link = 0x800000;
    }
}

bitflags! {
    #[repr(transparent)]
    #[derive(BinRead, BinWrite, Clone, Copy, Default, Debug)]
    #[br(map=|x| Self::from_bits_retain(x))]
    #[bw(map=|x: &Self| x.bits())]
    pub struct Attribute: u32 {
        const Earth = 1;
        const Water = 2;
        const Fire = 4;
        const Wind = 8;
        const Light = 16;
        const Dark = 32;
        const Devine = 64;
    }
}


bitflags! {
    #[repr(transparent)]
    #[derive(BinRead, BinWrite, Clone, Copy, Default, Debug)]
    #[br(map=|x| Self::from_bits_retain(x))]
    #[bw(map=|x: &Self| x.bits())]
    pub struct Linkmarkers: u32 {
        const BottomLeft = 1;
        const Bottom = 2;
        const BottomRight = 4;
        const Left = 8;
        const Right = 32;
        const TopLeft = 64;
        const Top = 128;
        const TopRight = 256;
    }
}

#[derive(BinRead, BinWrite, Copy, Clone, Eq, PartialEq, TryFromPrimitive, IntoPrimitive, Debug)]
#[brw(repr=u8)]
#[repr(u8)]
pub enum Duelstage {
    Begin = 0,
    Finger = 1,
    Firstgo = 2,
    Dueling = 3,
    Siding = 4,
    End = 5,
}

#[derive(BinRead, BinWrite, Copy, Clone, Eq, PartialEq, TryFromPrimitive, IntoPrimitive, Debug)]
#[brw(repr=u8)]
#[repr(u8)]
pub enum Colors {
    Observer = 7,
    Lightblue = 8,
    Red = 11,
    Green = 12,
    Blue = 13,
    Babyblue = 14,
    Pink = 15,
    Yellow = 16,
    White = 17,
    Gray = 18,
    Darkgray = 19,
}

impl std::default::Default for Colors {
    fn default() -> Self {
        return Colors::Observer;
    }
}

#[derive(BinRead, BinWrite, Copy, Clone, Eq, PartialEq, TryFromPrimitive, IntoPrimitive, Debug)]
#[brw(repr=u8)]
#[repr(u8)]
pub enum Hint {
    Event = 1,
    Message = 2,
    SelectMessage = 3,
    Opselected = 4,
    Effect = 5,
    Race = 6,
    Attribute = 7,
    Code = 8,
    Number = 9,
    Card = 10,
    Zone = 11,
}

#[derive(BinRead, BinWrite, Copy, Clone, Eq, PartialEq, TryFromPrimitive, IntoPrimitive, Debug)]
#[brw(repr=u16)]
#[repr(u16)]
pub enum Phase {
    Draw = 1,
    Standby = 2,
    Main1 = 4,
    BattleStart = 8,
    BattleStep = 16,
    Damage = 32,
    DamageCalculate = 64,
    Battle = 128,
    Main2 = 256,
    End = 512,
}


bitflags! {
    #[repr(transparent)]
    #[derive(BinRead, BinWrite, Clone, Copy, Default, Debug)]
    #[br(map=|x| Self::from_bits_retain(x))]
    #[bw(map=|x: &Self| x.bits())]
    pub struct SummonType: u32 {
        const Normal = 0x10000000;
        const Advance = 0x11000000;
        const Dual = 0x12000000;
        const Flip = 0x20000000;
        const Special = 0x40000000;
        const Fusion = 0x43000000;
        const Ritual = 0x45000000;
        const Synchro = 0x46000000;
        const Xyz = 0x49000000;
        const Pendulum = 0x4a000000;
        const Link = 0x4c000000;
    }
}

#[derive(BinRead, BinWrite, Copy, Clone, Eq, PartialEq, TryFromPrimitive, IntoPrimitive, Debug)]
#[brw(repr=u8)]
#[repr(u8)]
pub enum Hand {
    Scissor = 1,
    Rock = 2,
    Paper = 3
}


bitflags! {
    #[repr(transparent)]
    #[derive(BinRead, BinWrite, Clone, Copy, Default, Debug)]
    #[br(map=|x| Self::from_bits_retain(x))]
    #[bw(map=|x: &Self| x.bits())]
    pub struct OT: u32 {
        const OCG = 0x1;
        const TCG = 0x2;
    }
}

bitflags! {
    #[repr(transparent)]
    #[derive(BinRead, BinWrite, Clone, Copy, Default, Debug)]
    #[br(map=|x| Self::from_bits_retain(x))]
    #[bw(map=|x: &Self| x.bits())]
    pub struct Category: u32 {
        const CATEGORY_1 = 0x1;
        const CATEGORY_2 = 0x2;
        const CATEGORY_3 = 0x4;
        const CATEGORY_4 = 0x8;
        const CATEGORY_5 = 0x10;
        const CATEGORY_6 = 0x20;
        const CATEGORY_7 = 0x40;
        const CATEGORY_8 = 0x80;
        const CATEGORY_9 = 0x100;
        const CATEGORY_10 = 0x200;
        const CATEGORY_11 = 0x400;
        const CATEGORY_12 = 0x800;
        const CATEGORY_13 = 0x1000;
        const CATEGORY_14 = 0x2000;
        const CATEGORY_15 = 0x4000;
        const CATEGORY_16 = 0x8000;
        const CATEGORY_17 = 0x10000;
        const CATEGORY_18 = 0x20000;
        const CATEGORY_19 = 0x40000;
        const CATEGORY_20 = 0x80000;
        const CATEGORY_21 = 0x100000;
        const CATEGORY_22 = 0x200000;
        const CATEGORY_23 = 0x400000;
        const CATEGORY_24 = 0x800000;
        const CATEGORY_25 = 0x1000000;
        const CATEGORY_26 = 0x2000000;
        const CATEGORY_27 = 0x4000000;
        const CATEGORY_28 = 0x8000000;
        const CATEGORY_29 = 0x10000000;
        const CATEGORY_30 = 0x20000000;
        const CATEGORY_31 = 0x40000000;
        const CATEGORY_32 = 0x80000000;
    }
}

#[derive(BinRead, BinWrite, Copy, Clone, Eq, PartialEq, TryFromPrimitive, IntoPrimitive, Debug)]
#[brw(repr=u32)]
#[repr(u32)]
pub enum OperationCode {
    Add = 0x40000000,
    Subtract = 0x40000001,
    Multiply = 0x40000002,
    Divide = 0x40000003,
    And = 0x40000004,
    Or  = 0x40000005,
    Negate = 0x40000006,
    Not = 0x40000007,
    IsCode = 0x40000100,
    IsSetcard = 0x40000101,
    IsType = 0x40000102,
    IsRace = 0x40000103,
    IsAttribute = 0x40000104,
}

#[derive(BinRead, BinWrite, Copy, Clone, Eq, PartialEq, TryFromPrimitive, IntoPrimitive, Debug)]
#[brw(repr=u8)]
#[repr(u8)]
pub enum MasterRule {
    MasterRule1 = 1,
    MasterRule2 = 2,
    MasterRule3 = 3,
    MasterRuleNew = 4,
    MasterRule2020 = 5,
}

#[derive(BinRead, BinWrite, Copy, Clone, Eq, PartialEq, TryFromPrimitive, IntoPrimitive, Debug)]
#[brw(repr=u8)]
#[repr(u8)]
pub enum Activity {
    Summon = 1,
    NormalSummon = 2,
    SpecialSummon = 3,
    FlipSummon = 4,
    Attack = 5,
    BattlePhase = 6,
    Chain = 7,
}

#[derive(BinRead, BinWrite, Copy, Clone, Eq, PartialEq, TryFromPrimitive, IntoPrimitive, Debug)]
#[brw(repr=u8)]
#[repr(u8)]
pub enum CardHint {
    Turn = 1,
    Card = 2,
    Race = 3,
    Attribute = 4,
    Number = 5,
    DescriptionAdd = 6,
    DescriptionRemove = 7,
}

#[derive(BinRead, BinWrite, Copy, Clone, Eq, PartialEq, TryFromPrimitive, IntoPrimitive, Debug)]
#[brw(repr=u8)]
#[repr(u8)]
pub enum PlayerHint {
    DescriptionAdd = 6,
    DescriptionRemove = 7,
}

#[derive(BinRead, BinWrite, Copy, Clone, Eq, PartialEq, TryFromPrimitive, IntoPrimitive, Debug)]
#[brw(repr=u8)]
#[repr(u8)]
pub enum EffectDescription {
    Operation = 1,
    Reset = 2,
}

#[derive(BinRead, BinWrite, Copy, Clone, Eq, PartialEq, TryFromPrimitive, IntoPrimitive, Debug)]
#[brw(repr=i8)]
#[repr(i8)]
pub enum OperationResult {
    Canceled = -1,
    Fail = 0,
    Success = 1,
}
