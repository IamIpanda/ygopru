# ygopro

The YGOPro duel server, rewritten in Rust: it hosts duels and speaks the YGOPro binary
protocol. This crate also ships the `ygopro` binary — for running the binary (its
arguments and the data files it needs, such as `cards.cdb` and `script`), see the
[usage section of the ygopru repository README](https://github.com/iamipanda/ygopru#usage).

The duelling engine itself is still the battle-tested
[ygopro-core](https://github.com/mycard/ygopro-core) (ocgcore), linked through the FFI
wrapper [`ygopro-core-wrapper`](https://docs.rs/ygopro-core-wrapper). Everything around
it — networking, message handling, rooms, replay — is implemented here and in its
sibling crates [`ygopro-data`](https://docs.rs/ygopro-data) (the wire protocol model),
[`ygopro-handler`](https://docs.rs/ygopro-handler) (the message dispatcher) and
[`ygopro-derive`](https://docs.rs/ygopro-derive) (the handler/plugin macros).

This crate provides the complete duel logic, and the server's behaviour can be
customized by writing plugins.

## Example: a duel server with a custom plugin

A plugin is a module of handlers that react to protocol messages. This one greets a
player the moment they join a duel:

```rust,no_run
// welcome.rs
use ygopro::ygopro_handlers::Handler;
use ygopro::ygopro_handlers::YGOPRO_HANDLERS;
use ygopro_data::constants::Color;
use ygopro_data::message::ctos;
use ygopro_data::message::stoc;
use ygopro_derive::after;
use ygopro_derive::register_to;

pub static NAME: &'static str = module_path!();

#[after(ctos::JoinGame)]
#[register_to(YGOPRO_HANDLERS)]
fn on_join_game() -> stoc::Message {
    stoc::Chat { player: Color::Red.into(), msg: "Hello, world!".into() }.into()
}
```

A plugin only takes effect once enabled by its module path. Build a [`DuelHost`] from
a `HostInfo` and a [`struct@Configuration`], enable the plugin, and serve one duel over
TCP:

```rust,no_run
// main.rs
mod welcome;

use ygopro::cli::start_local_server;
use ygopro::host::DuelHost;
use ygopro::Configuration;
use ygopro_data::message::HostInfo;

#[tokio::main]
async fn main() {
    ygopro::init();

    let mut configuration = Configuration::default();
    configuration.enable_plugin(welcome::NAME);

    let duel_host = DuelHost::new(HostInfo::default(), configuration);
    start_local_server(7911, duel_host).await;
}
```

## Initialization

Before hosting duels, call [`ygopro::init`] once — the example above does it at the
start of `main`. It loads the global managers the engine draws on: card data, limit
lists, string tables and the server configuration.

```rust
ygopro::init();
```

Before it can load anything, make sure the following files are put in the current
directory:

- `cards.cdb` — you can find one from
  [ygopro-database](https://github.com/mycard/ygopro-database).
- `script` folder — you can download from
  [ygopro-scripts](https://github.com/Fluorohydride/ygopro-scripts).
- `lflist.conf` — this file can be skipped if you don't need a ban/limit list.
- `system.conf` and `strings.conf` are not needed.

[`ygopro::init`]: https://docs.rs/ygopro/latest/ygopro/fn.init.html
