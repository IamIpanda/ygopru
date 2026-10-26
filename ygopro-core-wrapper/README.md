# ygopro-core-wrapper

Rust FFI bindings to [ygopro-core](https://github.com/mycard/ygopro-core) (`ocgcore`), the duel
engine behind YGOPro.

`ocgcore` and `lua` are vendored as git submodules and compiled by `build.rs` through `cc`, with
the same language settings as the upstream C++ build. The lua C API is exported unmangled, so
`mlua` binds to the very `lua_State` a duel already owns, instead of opening a second interpreter.

Initialize the submodules before the first build:

```sh
git submodule update --init --recursive
```

## Features

Two independent choices have to be made: which core to link, and which lua version to compile.

### Core

Exactly one core is linked, unless `dual` is enabled.

| Feature | Meaning |
| --- | --- |
| `official` | Link `mycard/ygopro-core` (`core/official`). This is the default. |
| `koishi` | Link `purerosefallen/ygopro-core` (`core/koishi`). |
| `custom` | Link an ocgcore source tree outside this repository, given by the environment variable `YGOPRO_OCGCORE_DIR`. |
| `dual` | Link both `official` and `koishi` in one binary. |

The single-core features are mutually exclusive. Enabling more than one of them still builds, but
only one core is used, chosen by the priority `custom` > `koishi` > `official`, and a build warning
is emitted. `dual` takes precedence over all of them.

The two cores export the same C symbols, so a `dual` build renames every symbol of the koishi
archive to carry a `koishi_` prefix before linking. That makes both cores addressable at once:

| Build | Public API |
| --- | --- |
| single core | Core functions are re-exported at the crate root, `Duel::new(seed)`. |
| `dual` | The `official` and `koishi` modules are exposed, each holding one set of core functions. `Duel::new(seed, kind)` selects the core with a `CoreKind`. |

### Lua

Exactly one lua version must be selected. `lua54` is the default.

| Feature | Meaning |
| --- | --- |
| `lua54` | Compile `lua/5.4.8`. |
| `lua55` | Compile `lua/5.5.1`. |

Only the selected tree is compiled; the other one stays untouched on disk. Selecting neither one
is a build error reported by `mlua`, because it cannot pick a lua ABI without being told.

### Others

| Feature | Meaning |
| --- | --- |
| `card` | Forward to `ygopro-data/card`, which enables card database support. |

## Examples

```rust,no_run
use ygopro_core_wrapper::Duel;
use ygopro_core_wrapper::DuelSeed;

let duel = Duel::new(DuelSeed::Single(42));
```

With `dual`, the core is chosen at construction:

```rust,no_run
use ygopro_core_wrapper::CoreKind;
use ygopro_core_wrapper::Duel;
use ygopro_core_wrapper::DuelSeed;

let duel = Duel::new(DuelSeed::Single(42), CoreKind::Koishi);
```
