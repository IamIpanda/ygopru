# Variables

`system.conf` is optional. The engine and the plugins it runs read their configuration values from it, and when
the file is missing every entry keeps its default. Its location is the file `YGOPRO_CONFIG_PATH` names, default
`system.conf`.

An entry is a `key = value` line and `#` starts a comment. The same key uppercased with a `YGOPRO_` prefix is an
environment variable that overrides the file. The engine itself takes the bare keys:

| Variable | Default | Description |
| -------- | ------- | ----------- |
| `YGOPRO_PATH` | `./` | Base directory card scripts are resolved against |
| `YGOPRO_DB_PATH` | `cards.cdb, expansions/*.cdb` | Card databases, comma-separated, globs allowed |
| `YGOPRO_LFLIST_PATH` | `expansions/lflist.conf, lflist.conf` | Ban lists, comma-separated, globs allowed |
| `YGOPRO_PREFER_EXPANSION_SCRIPT` | `false` | Look a card script up in `expansions` before the base directory |

## Plugins

A plugin owns the entries prefixed with its own name. 

Enabling / Disabling Plugin must be from code.

Most plugins are designed to be enabled, disabled and configured from code, which is more flexible. Some entries cannot be loaded from the file because of their complexity.

| Plugin | Need Enabling | Variable | Default | Description |
| ------ | ------------- | -------- | ------- | ----------- |
| `bo` | yes | `YGOPRO_BO_OVERRIDE_BEST_OF` | `0` | Duels in a best-of-N match; `0` keeps the Single/Match behaviour of the room |
| `replay` | no | `YGOPRO_REPLAY_SAVE_PATH` | `replays` | Directory saved replays are written to |
| `replay` | no | `YGOPRO_REPLAY_FORMAT` | `ygopro` | `ygopro`, `forge` or `srvpro` |
| `replay` | no | `YGOPRO_REPLAY_FILE_TEMPLATE` | `{time} {players} Duel {duel}` | Replay file name, placeholders filled from the duel |
| `replay` | no | `YGOPRO_REPLAY_TIME_FORMAT` | `%Y-%m-%d %H-%M-%S` | `strftime` pattern used for `{time}` |
| `replay` | no | `YGOPRO_REPLAY_MACHINE` | `0` | Machine number a saved replay id is built from |
| `terminate` | no | `YGOPRO_TERMINATE_TERMINATE_WHEN_MATCH_END` | `true` | Terminate the room when the match ends, dont wait the remaining clients |
| `time_backed` | no | `YGOPRO_TIME_BACKED_ADD_TIME_AFTER_OPERATION` | `1` | When player operate, add extra time to that player. |
| `time_backed` | no | `YGOPRO_TIME_BACKED_MAX_ADD_TIME_EACH_TURN` | `0` | Max time can be added per turn; `0` takes the room's time limit |
| `time_compensator` | no | `YGOPRO_TIME_COMPENSATOR_ADD_SMALL_TIME_DEPOSIT_AFTER_OPERATION` | `1` | Seconds added to the compensation deposit per operation |
| `time_compensator` | no | `YGOPRO_TIME_COMPENSATOR_IGNORE_SMALL_TIME_UNDER_THIS_DURATION` | `10` | Elapsed time under this many seconds is ignored, funded by the deposit |
| `time_compensator` | no | `YGOPRO_TIME_COMPENSATOR_INITIAL_TIME_BALANCE` | `0` | Deposit at the start of each turn; `-1` takes the room's time limit |

`time_compensator` behaves differently from the origin ygopro; see
[its plugin documentation](https://docs.rs/ygopro/latest/ygopro/plugin/time_compensator/index.html#warning).

