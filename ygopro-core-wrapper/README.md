# ygopro-core-wrapper

Rust FFI wrapper for [ygopro-core](https://github.com/mycard/ygopro-core) (ocgcore).

## Prerequisites

- C++14 compiler (gcc/clang/MSVC)
- Rust toolchain (stable)

## Building

```bash
# First clone submodules
git submodule update --init

# Then build
cargo build -p ygopro-core-wrapper
```

This will:
1. Compile Lua from `ygopro-core-wrapper/lua/` as a static library
2. Compile ocgcore from `ygopro-core-wrapper/ocgcore/` as a static library (C++14)
3. Link both into the Rust crate

## Alternative: Build with premake5

If you prefer to use premake5 to build ocgcore independently:

```bash
cd ygopro-core-wrapper/ocgcore
premake5 gmake2
make -C build
```

Then link the resulting static library manually.

## API

All functions from `ocgapi.h` are exported as unsafe FFI functions:

- `create_duel` / `create_duel_v2` / `create_duel_safe` - Create a new duel instance
- `start_duel` / `start_duel_with_rule` - Start the duel
- `end_duel` - End and clean up
- `process` - Process duel state machine
- `get_message` - Get pending engine messages
- `set_responsei` / `set_responseb` - Send player response
- `new_card` / `new_tag_card` - Add cards to duel
- `query_card` / `query_field_card` / `query_field_count` - Query game state
- `set_player_info` - Set player LP/hand/draw
- `set_script_reader` / `set_card_reader` / `set_message_handler` - Set callbacks
- `preload_script` / `preload_script_from_path` - Load Lua scripts`
