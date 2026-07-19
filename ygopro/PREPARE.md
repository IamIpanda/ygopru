# 匹配阶段 (DUEL_STAGE_BEGIN) CTOS → STOC 对应表

| CTOS 消息 | 条件 | STOC 消息 | 接收者 | 状态变化 |
|-----------|------|-----------|--------|----------|
| **PlayerInfo** (16) | — | *(无)* | — | `dp->name = name` |
| **CreateGame** (17) | — | STOC_JOIN_GAME | `dp` | `dp->game = this`; `host_player = dp`; `players[0] = dp`; `dp->type = PLAYER1` |
| | | STOC_TYPE_CHANGE | `dp` | |
| **JoinGame** (18) | 版本不对 | STOC_ERROR_MSG (VERERROR) | `dp`，随后断连 | 断连 |
| | 已在其他房间 | STOC_ERROR_MSG (JOINERROR, 0) | `dp`，随后断连 | 断连 |
| | 密码错误 | STOC_ERROR_MSG (JOINERROR, 1) | `dp` | *(无)* |
| | 加入为玩家 | STOC_HS_PLAYER_ENTER（含 dp 名字+位置） | all except dp | `dp->game = this`; 若第一人则 `host_player = dp`; `players[空位] = dp`; `dp->type = PLAYER1/PLAYER2` |
| | | STOC_JOIN_GAME | `dp` | |
| | | STOC_TYPE_CHANGE | `dp` | |
| | | STOC_HS_PLAYER_ENTER（各已在场玩家名字+位置，每玩家一条） | `dp` | |
| | | STOC_HS_PLAYER_CHANGE (READY)（各已 ready 玩家，每 ready 玩家一条） | `dp` | |
| | | STOC_HS_WATCH_CHANGE（若有观察者） | `dp` | |
| | 加入为观察者 | STOC_HS_WATCH_CHANGE | `all` | `dp->game = this`; `observers.insert(dp)`; `dp->type = OBSERVER` |
| | | STOC_JOIN_GAME | `dp` | |
| | | STOC_TYPE_CHANGE | `dp` | |
| | | STOC_HS_PLAYER_ENTER（各已在场玩家名字+位置，每玩家一条） | `dp` | |
| | | STOC_HS_PLAYER_CHANGE (READY)（各已 ready 玩家，每 ready 玩家一条） | `dp` | |
| | | STOC_HS_WATCH_CHANGE | `dp` | |
| **UpdateDeck** (2) | 无效卡组 | STOC_HS_PLAYER_CHANGE (NOTREADY)（仅 YGOPRO_SERVER_MODE） | `dp` | *(无)* |
| | | STOC_ERROR_MSG (DECKERROR, 0) | `dp` | |
| | 有效卡组 | *(无)* | — | `LoadDeck`; 记录 `deck_error[dp->type]`; 服务端模式自动 `ready[dp->type] = true` |
| **HsToDuelist** (32) | — | STOC_HS_PLAYER_ENTER | `all` | `observers.erase(dp)`; `players[空位] = dp`; `dp->type = PLAYER1/PLAYER2` |
| | | STOC_HS_WATCH_CHANGE | `all` | |
| | | STOC_TYPE_CHANGE | `dp` | |
| **HsToObserver** (33) | — | STOC_HS_PLAYER_CHANGE (OBSERVE) | `all` | `players[dp->type] = null`; `ready[dp->type] = false`; `dp->type = OBSERVER`; `observers.insert(dp)` |
| | | STOC_TYPE_CHANGE | `dp` | |
| **HsReady** (34) | 卡组校验失败 | STOC_HS_PLAYER_CHANGE (NOTREADY) | `dp` | *(ready 仍为 false)* |
| | | STOC_ERROR_MSG (DECKERROR) | `dp` | |
| | 校验通过 | STOC_HS_PLAYER_CHANGE (READY) | `all` | `ready[dp->type] = true` |
| **HsNotReady** (35) | — | STOC_HS_PLAYER_CHANGE (NOTREADY) | `all` | `ready[dp->type] = false` |
| **HsKick** (36) | — | STOC_HS_PLAYER_CHANGE (LEAVE) | all except 被踢者 | `players[被踢者] = null`; `ready[被踢者] = false`; 被踢者断连 |
| **HsStart** (37) | — | STOC_DUEL_START | `all` | `StopListen()`; `duel_stage = FINGER`; 双方 `state = CTOS_HAND_RESULT`; 观察者 `state = CTOS_LEAVE_GAME` |
| | | STOC_DECK_COUNT | `dp + 1-dp` | |
| | | STOC_SELECT_HAND | `dp + 1-dp` | |

> **接收者**：`dp` = 发起 CTOS 的玩家；`1-dp` = 对手玩家；`all` = 双方 + 所有观察者；`all except dp` = 除发起者外所有人；`all except 被踢者` = 除被踢玩家外所有人。
