# ygopro-core-wrapper

Rust FFI bindings to [ygopro-core](https://github.com/mycard/ygopro-core) (`ocgcore`), the duel
engine behind YGOPro.

`ocgcore` and `lua` are vendored as git submodules and compiled by `build.rs` through `cc`, with
the same language settings as the upstream C++ build. 

## Features

Besides plain FFI bindings, the crate has a few extra capabilities:

- The `lua_State` a duel owns is exposed through `mlua`, so the caller can run lua code against the
  running duel instead of opening a second interpreter. See [`Duel::lua`].
- The shuffler of the original core is exposed, so a random sequence can be reproduced outside the
  duel and stays strictly identical to what the C++ implementation produces. See
  [`MTRandom`](random::MTRandom).
- Several cores can be linked and used at the same time. See [Mix mode](#mix-mode).

## Cargo features

Two independent choices have to be made: which core to link, and which lua version to compile.

### Core

**At least one core must be selected**; selecting none is a build error.

| Feature | Meaning |
| --- | --- |
| `official` | Link `mycard/ygopro-core` (`core/official`). This is the default. |
| `koishi` | Link `purerosefallen/ygopro-core` (`core/koishi`). |
| `custom` | Link an ocgcore source tree outside this repository. See [Custom core](#custom-core). |

With one core, its functions are re-exported at the crate root.

### Mix mode

Selecting more than one core links all of them into one binary and switches the crate into mix
mode. Nothing is re-exported at the crate root any more; every linked core becomes a module, and
`Duel::new` asks which one to use.

- `CoreKind` only carries the variants that are actually linked. `CoreKind::default()` picks
  `Official`, then `Koishi`, then `Custom`, whichever exists.
- The cores hold identical C symbol names, so calling them at the crate root would be ambiguous.
  Use `ygopro_core_wrapper::official::set_script_reader`,
  `ygopro_core_wrapper::koishi::set_script_reader`, and the same for every other entry point.
  Callbacks are per core: each one has to be installed separately.
- This crate sees the mix state as `cfg(mix_core)`.

Symbol collisions are resolved at link time with the `rust-objcopy` shipped in the Rust toolchain,
through `--redefine-syms`. `official` keeps the original symbol names, `koishi` is prefixed with
`koishi_`, and `custom` with `custom_`.

Crates depending on this one read the `DEP_YGOPRO_CORE_MIX_CORE` build-script metadata.

### Custom core

`custom` ships no bindings. It expects the builder to provide them.

| Variable | Meaning |
| --- | --- |
| `YGOCORE_CUSTOM_DIR` | ocgcore source tree to compile. Required when `custom` is enabled. |
| `YGOCORE_CUSTOM_WRAPPER` | Rust file replacing the built-in `src/custom.rs`. |
| `YGOCORE_OFFICIAL_DIR` | ocgcore source tree compiled for `official`. Defaults to `core/official`. |
| `YGOCORE_KOISHI_DIR` | ocgcore source tree compiled for `koishi`. Defaults to `core/koishi`. |

The bindings file becomes the body of the `custom` module. The file must expose:

```rust,ignore
pub(crate) fn api() -> crate::ygocore::ygocore
```

Nothing else is assumed. The file may declare any symbols in any shape, as long as they fill in the
function pointer table; `crate::ygocore::ygocore` is crate-visible because the file is compiled as
part of this crate.

When `custom` takes part in a mix, its archive is renamed to the `custom_` prefix, so an external
wrapper file has to spell that prefix out in its own `link_name` attributes. The built-in
`src/custom.rs` does it with `#[cfg_attr(mix_core, link_name = "custom_...")]`.

### Lua

Exactly one lua version must be selected. `lua54` is the default.

| Feature | Meaning |
| --- | --- |
| `lua54` | Compile `lua/5.4.8`. |
| `lua55` | Compile `lua/5.5.1`. |

Only the selected tree is compiled; the other one stays untouched on disk. Selecting neither one
is a build error reported by `mlua`, because it cannot pick a lua ABI without being told.

## Examples

One core selected:

```rust,no_run
use ygopro_core_wrapper::Duel;
use ygopro_core_wrapper::DuelSeed;

let duel = Duel::new(DuelSeed::Single(42));
```

More than one core selected:

```rust,no_run
use ygopro_core_wrapper::CoreKind;
use ygopro_core_wrapper::Duel;
use ygopro_core_wrapper::DuelSeed;

let duel = Duel::new(DuelSeed::Single(42), CoreKind::Koishi);
```
