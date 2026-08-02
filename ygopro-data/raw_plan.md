# ygopro-data raw（unsafe）方案

目标：给 `ygopro-data` 添加一套 unsafe 的、内存结构严格对应原版 C++ ygopro 的类型系统，
并与当前 safe（binrw）版本共存于同一个包里。

对应原版：`../ygopro/gframe/network.h`、`../ygopro/gframe/replay.h`、
`../ygopro/gframe/bufferio.h`、`../ygopro/ocgcore/card_data.h`、`../ygopro/gframe/deck.h`。

## 发版与打包

- 单一包 `ygopro-data` 同时发布两个版本（safe + raw）。
- 模块划分：
  - `constants.rs`、`data/` —— 共享，只保留一份。
  - `message/` —— safe（binrw）版本，维持现状。
  - `raw/`（新建）—— unsafe 内存严格版本。
- 两套**常驻编译**，不做 feature 开关（避免 `cfg` 分裂 / feature 组合爆炸）。
  具体用哪套由下游 crate 显式选择。

## 原则

- 所有线上结构体用 `#[repr(C)]`；C++ 侧 `#pragma pack(1)` 的地方用
  `#[repr(C, packed)]`（只有 `STOC_HS_PlayerEnter`）。
- 从字节缓冲读写一律用 `ptr::read_unaligned` / `ptr::write_unaligned` /
  `copy_nonoverlapping`。**严禁**把 `&[u8]` 直接 cast 成 `&T`。
- 每个定长结构体带编译期尺寸断言，对照 C++ 的 `static_assert`。
- 仅小端（x86 目标）；要跨平台就用 `from_le_bytes`。
- 网络字符串是 `uint16_t[]`（UTF-16LE），不是 `wchar_t`。

## 共享，零改动

### constants.rs
所有语义 enum / bitflags 已经 repr 正确，两套共用。不改。

- repr(u8)：`Netplayer`（带数据）、`CorePlayer`、`Mode`、`JoinError`、
  `DuelStage`、`Colors`、`Hint`、`Hand`、`MasterRule`、`Activity`、`CardHint`、
  `PlayerHint`、`EffectDescription`、`WinReason`、`SelectSumMode`。
- repr(u16)：`Network`、`Phase`。
- repr(i8)：`OperationResult`。
- repr(transparent) bitflags：`Location`(u8)、`Position`(u8)、`Rule`(u8)、
  `Timing`、`Type`、`Race`、`Reason`、`Status`、`Query`、`Attribute`、
  `Linkmarkers`、`SummonType`、`Category`（u32）。
- modular_bitfield `#[repr(u8)]`：`TypeChange`、`PlayerChange`。
  字段位序是低位在前，与 C++ 位移一致（`type<<4|host`、`type<<4|state`）。
- `ErrorMessage`、`HandResult`（judge 枚举）是语义类型，非线上结构。

### data/card.rs
- `CoreCard` —— 已是 `#[repr(C)]`，80 字节，`validate_core_card_raw_bytes`
  逐字节验证过。不改。
- `Card` —— 内存模型（字符串 + 数据库行）。不改。

### data/deck.rs
- `Deck`、`ReplayDeck` —— 内存模型（`Vec<u32>`）。不改。
- `DeckError`（modular_bitfield `#[repr(u32)]`）—— 已正确。

### data/lflist.rs、data/strings.rs
纯内存文本解析。不改。

## 定长线上结构（新建 `raw`）

写成 `#[repr(C)]` + `Copy` + 尺寸断言。以下 offset 是相对 payload（proto 字节之后）。

### network.h

```
HostInfo             20B  lflist:u32[0] rule:u8[4] mode:u8[5] duel_rule:u8[6]
                           no_check_deck:u8[7] no_shuffle_deck:u8[8] pad[9..12]
                           start_lp:i32[12] start_hand:u8[16] draw_count:u8[17]
                           time_limit:u16[18]
CTOS_HandResult       1B  res:u8
CTOS_TPResult         1B  res:u8
CTOS_PlayerInfo      40B  name:[u16;20]
CTOS_CreateGame     100B  info:HostInfo name:[u16;20] pass:[u16;20]
CTOS_JoinGame        48B  version:u16[0] pad[2..4] gameid:u32[4] pass:[u16;20][8..48]
CTOS_Kick             1B  pos:u8
STOC_ErrorMsg         8B  msg:u8[0] pad[1..4] code:u32[4]
STOC_HandResult       2B  res1:u8 res2:u8
STOC_JoinGame        20B  info:HostInfo
STOC_TypeChange       1B  type:u8            （bit4 = host）
STOC_ExitGame         1B  pos:u8
STOC_TimeLimit        4B  player:u8[0] pad[1..2] left_time:u16[2]
STOC_HS_PlayerEnter  41B  name:[u16;20] pos:u8          -> repr(C, packed)
STOC_HS_PlayerChange  1B  status:u8
STOC_HS_WatchChange   2B  watch_count:u16
STOC_DeckCount       12B  [i16;6]
```

零字节消息（无结构体）：CTOS LeaveGame / Surrender / TimeConfirm / HsToDuelist /
HsToObserver / HsReady / HsNotReady / HsStart / RequestField；STOC SelectHand /
SelectTp / ChangeSide / WaitingSide / DuelStart / DuelEnd / FieldFinish /
TeammateSurrender；STOC_TpResult / STOC_CreateGame（保留）。

### replay.h

```
ReplayHeader         32B  id:u32 version:u32 flag:u32 seed:u32 datasize:u32
                           start_time:u32 props:[u8;8]
ExtendedReplayHeader 80B  base:ReplayHeader(32) seed_sequence:[u32;8](32)
                           header_version:u32 value1:u32 value2:u32 value3:u32
DuelParameters       16B  start_lp:i32 start_hand:i32 draw_count:i32 duel_flag:u32
```

raw 版 replay 头固定是 80 字节的 `ExtendedReplayHeader`（C++ 无条件写入）。
V1 回放只是尾部留零。当前 safe 版 `ReplayHeader` 的 `br(if V2)` 条件只是
safe 层的关心点。

### card_data.h

`card_data` = 80B = 现有 `CoreCard`。已完成。

### message/utils.rs 的 `HostInfo`

线上版本就是上面那个 20B 结构。去掉 `br/bw(map)` 的 bool 映射，保留 `u8` 字段，
在访问层做语义转换。

## 位压缩字段

- `CardCode`（u32：B28 id + 3 保留 + 1 is_public）—— modular_bitfield
  `#[repr(u32)]`，线上就是 4 字节。保留。
- `DeckError`（u32：4bit 类型 + B28 code）—— 保留。
- `TypeChange`、`PlayerChange` —— 保留（见 constants.rs）。
- `ctos::HandResult` / `gm::HandResult` 打包：gm `HandResult` 把两个 `Hand`(2bit)
  打包进一个 u8（`res1 | res2<<2`）。raw：读 u8，在访问层解包。

## 变长消息

C 里没有结构体，线上就是字节流，用 C++ `BufferIO` 的方式消费。两种 raw 形态：

1. 定长头 + 尾随 slice（`slice::from_raw_parts`，零拷贝视图）。
2. 游标读（`*const u8` + len），用 `read_unaligned` 逐字段推进。

### ctos

```
Response   Vec<u8> until_eof                     -> 形态 2，原始字节切片
UpdateDeck u32 mainc, u32 sidec, u32[mainc+sidec] -> 形态 1（DeckWireHeader 8B + codes）
Chat       u16[] until_eof                       -> 形态 2，UTF-16 切片
```

### stoc

```
GameMessage  内嵌 gm 流                         -> 形态 2（委托给 gm）
Chat         u16 player_type, u16[] msg          -> 形态 2
Replay       ExtendedReplayHeader + lzma body    -> replay 层
```

### game_message（94 个消息）

- 约 58 个定长 —— `#[repr(C)]` + `Copy` + 尺寸断言。例如：
  Retry(0)、Hint(6)、Waiting(0)、Start(19)、Win(2)、Move(16)、
  PositionChange(9)、Set(8)、Swap(16)、FieldDisabled(4)、Summoning(8)、
  Summoned(0)、SpecialSummoning(8)、SpecialSummoned(0)、FlipSummoning(8)、
  FlipSummoned(0)、Chaining(17)、Chained(1)、ChainSolving(1)、ChainSolved(1)、
  ChainNegated(1)、ChainDisabled(1)、ChainEnd(0)、Damage(5)、Recover(5)、
  Equip(8)、LPUpdate(5)、Unequip(4)、CardTarget(8)、CancelTarget(8)、
  PayLPCost(5)、AddCounter(7)、RemoveCounter(7)、Attack(8)、Battle(26)、
  AttackDisabled(0)、DamageStepStart(0)、DamageStepEnd(0)、MissedEffect(8)、
  BeChainTarget(0)、CreateRelation(0)、ReleaseRelation(0)、
  RockPaperScissors(1)、AnnounceRace(6)、AnnounceAttribute(6)、CardHint(9)、
  PlayerHint(6)、MatchKill(4)、ReverseDeck(0)、ShuffleDeck(1)、RefreshDeck(1)、
  SwapGraveDeck(1)、NewTurn(1)、NewPhase(2)、DeckTop(6)、RequestDeck(0)。
- 约 42 个带 Vec / 条件字段 —— 形态 1 或 2：
  SelectBattleCommand、SelectIdleCommand（6 个 Vec）、SelectEffectYesNo、
  SelectYesNo、SelectOption、SelectCard、SelectChain、SelectPlace、
  SelectPosition、SelectTribute、SortChain、SelectCounter、SelectSum、
  SelectDisableField、SortCard、SelectUnselectCard、ConfirmDecktop、
  ConfirmCards、ShuffleHand、ShuffleExtra、ShuffleSetCard、CardSelected、
  RandomSelected、BecomeTarget、Draw、HandResult、AnnounceCard、AnnounceNumber、
  UpdateData、UpdateCard、TossCoin、TossDice、ReloadField、TagSwap、CustomMsg、
  AIName、ShowHint、RequestDeck。
  每个保留 `#[repr(C)]` 定长头（counts/flag 的 u8/u32），加一段尾随数组视图，
  用 count 字段决定 slice 长度。safe 层 60 处 `br(if|map|count)` 属性必须逐一
  落成具体的字节布局决策。
- 辅助类型：`InfoLocation`（4B，Overlay 条件字段）、
  `CardPosition<CODE,SUB,DESCRIPTION>`（按 const 参数变宽）、`Chain`(9B)、
  `CardCode`(u32)、`MzoneSlot`(1-3B)/`SzonaSlot`(1-2B)/`PlayerField`/`ChainLink`
  （ReloadField 内部数据）。

### data/query.rs

`UpdateCardInfo` = u32 len + u32 flag + 按 flag 顺序的字段；`QueryData` 是
按 flag 的枚举。raw 形态：游标读，字段顺序照抄 `QueryDatas::read_options`
（Code、Position、Alias、Type、Level、Rank、Attribute、Race、Attack、Defense、
BaseAttack、BaseDefense、Reason、ReasonCard、EquipCard、TargetCard、OverlayCard、
Counters、Owner、Status、LeftScale、RightScale、Link）。Owner 带 3 字节 pad。

## Mask（全部原地）

mask 是在线上缓冲上的原地字节覆写，和 C++ 一致：

- 刷新/更新消息：先把完整缓冲发给能看到的人，再原地清零隐藏的 code 字段，
  然后发给其余人（`RefreshMzone`/`RefreshSingle` 语义）。
- 每个携带隐藏字段的消息类型提供一个 `mask_in_place(&mut [u8])`。
  只覆写 code / 身份字段，保留 position/status 字节。
- 涉及消息：Move、Set、SpecialSummoning、Swap、UpdateData、UpdateCard、Draw、
  ShuffleHand、ShuffleExtra、SelectCard、SelectTribute、SelectUnselectCard、
  ConfirmCards、ConfirmDecktop、ConfirmExtraTop、DeckTop、TagSwap，
  以及其中 `CardCode`/`CardPosition` 的 code 字段。
- 由此去掉 typed `GameMessage::mask` 递归派生；raw 层删掉 `ygopro-derive`
  的 `mask.rs`。

## 分发层

用 raw 分发器替代 `generate_enum` 的 binrw magic 分发（`Message::read_le`）：
读 proto 字节，按该消息的最小/最大长度做边界检查，然后把字节切片交给对应的
raw 类型（对无需字段的消息直接以 `Complex` 式原始字节保存）。

## 字符串

- `FixedLengthString<L>`：去掉 `OnceLock<String>` 缓存字段，变成
  `#[repr(transparent)]` over `[u16; L]`。惰性 `String` 缓存移出线上类型
  （放到调用方 / 侧表）。
- `U16String`：safe 层维持 owned `Vec<u16>`；raw 层用 UTF-16LE 字节切片，
  按需转换。

## 验证

- 每个定长结构体的编译期 `size_of` 断言，对照 C++ 每个 `static_assert`。
- 每个消息类型的 round-trip 测试（raw -> bytes -> raw）。
- 对定长网络结构、replay 头、代表性 game message，用抓到的 C++ 包做
  golden 字节对比测试。
- 复用现有 `query.rs` 的 round-trip 测试作为 query 布局基线。
