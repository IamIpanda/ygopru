# ygopro 移植 TODO

> 对照 ../ygopro/gframe/ (YGOPRO_SERVER_MODE)

## 核心消息循环架构

两个 `ygopro_handler::processor::Processor`，共享同一个 `ServerState`。

### ServerState

```rust
struct ServerState {
    duel: SingleDuel,
    data_manager: Arc<DataManager>,
    deck_manager: Arc<DeckManager>,
    stoc_senders: [mpsc::UnboundedSender<stoc::Message>; 2],
    observer_broadcast: broadcast::Sender<stoc::Message>,
}
```

### CTOS Processor

```
TCP Client ──ctos::Message──▶ Player { player, message } ──▶ CTOS Processor ──▶ handlers
```

| 模板参数 | 类型 |
|----------|------|
| `Key` | `u8` |
| `Req` | `PlayerMessage` |
| `State` | `ServerState` |
| `Res` | `()` |
| `H` | `SyncHandler<PlayerMessage, ServerState, ()>` |

`PlayerMessage` 是具名 struct（元组不行——`CorePlayer` 和具体 `ctos::*` 类型各自独立实现 `FromRequest` 时，需要从 struct 的不同字段提取）：

```rust
struct PlayerMessage {
    player: CorePlayer,
    message: ctos::Message,
}

impl MessageKey<u8> for PlayerMessage { ... }  // → message.message_key()
impl From<PlayerMessage> for PlayerMessage { ... }  // identity
```

每个 TCP 连接一个 `PlayerMessage` stream。

### Game Message Processor

```
ocgcore::get_message(buffer) ──▶ gm::Message ──▶ Game Message Processor ──▶ handlers
```

| 模板参数 | 类型 |
|----------|------|
| `Key` | `u8` |
| `Req` | `gm::Message` |
| `State` | `ServerState` |
| `Res` | `()` |
| `H` | `SyncHandler<gm::Message, ServerState, ()>` |

`gm::Message` 已实现 `MessageKey<u8>` 和 `Into<gm::Message>`（identity）。

### 主循环数据流

```
engine.process()
    │
    ├─▶ engine.get_message(buffer) → gm::Message
    │       │
    │       └─▶ Game Message Processor.process(stream)
    │               handler 从 ServerState 拿 channel sender，推 stoc::Message 给玩家
    │
    ├─▶ engine.set_responsei/b()  ← 由 CTOS handler 触发
    │
    └─▶ 检查是否有玩家需要回应 → 等待 CTOS Processor 处理玩家输入
```

CTOS handler 示例（`on_update_deck`）：

```rust
fn on_update_deck(player: CorePlayer, deck: ctos::UpdateDeck) {
    // CorePlayer       ← FromRequest: 从 PlayerMessage.player 提取
    // ctos::UpdateDeck  ← FromRequest: 从 PlayerMessage.message match 提取
    // handler 通过闭包捕获 &mut ServerState，修改 duel、查 deck_manager.check_deck、
    // 往 stoc_senders[player] 写消息
}
```

Game Message handler 示例（`on_start`）：

```rust
fn on_start(start: gm::Start, server_state: &mut ServerState) {
    // gm::Start ← FromRequest: 从 gm::Message match 提取
    // 广播给双方玩家，mask 手牌/盖卡
}
```

## 基础设施

- [ ] `ServerState` struct
- [ ] `PlayerMessage` struct + `MessageKey` / `From` impl
- [ ] `CorePlayer::from_request` for `PlayerMessage`
- [ ] 每种 `ctos::*` 类型的 `from_request` for `PlayerMessage`
- [ ] `gm::*` 类型的 `from_request` for `gm::Message`
- [ ] CTOS Processor 实例化 + 注册 handlers
- [ ] Game Message Processor 实例化 + 注册 handlers
- [ ] main.rs 调用 DataManager::load_db 和 DeckManager::load_lflist
- [ ] Seed 生成（随机种子、MATCH 模式 pre-seed、CLI base64 种子解码）
- [ ] 卡组校验接入（on_hs_ready 调用 check_deck）
- [ ] 计时器（tick 检查 time_limit 超时 → 判负）
- [ ] common::DuelMode 补齐字段（observers、match_result、duel_count、replay 字段）

## 大功能

- [ ] Tag Duel（tag_duel.rs 全是注释，C++ 500+ 行待移植）
- [ ] Replay 录制（BeginRecord / WriteHeader / WriteData / EndRecord）
- [ ] RequestField（掉线重连时重建全场状态）
- [ ] 场地刷新（RefreshMzone / Szone / Hand / Grave / Extra / Removed / Single）
- [ ] Observer 列表管理（subscribe_observer 有，但缺 std::set 式维护）

## 不需要

- [x] Bot/CPU 对战（服务器模式不需要）
- [x] LAN 广播发现（服务器模式不需要）
