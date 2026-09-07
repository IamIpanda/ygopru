# ygopro-data

The Rust model of the ygopro data structures.

This crate mirrors the types used by the original C++ ygopro and its core, so it is
the shared data layer for the Rust ports.

# Modules

- `data` — outside game data: cards, decks, limit lists, replays; and the
  complicated structs inside messages: queries, responses.
- `message` — ygopro protocol messages (client-to-server, server-to-client) and
  ygocore protocol messages (game message).
- `constants` — the constants and enums shared across the protocol
  (players, locations, ...).
- `utils` — UTF-16 string helpers and the lazy `Complex` message type.

# Notes

Some types and functions whose purpose is obvious from their name are left
undocumented.

Most plain types are byte-for-byte identical to ygopro. However, the following types
have a memory layout that differs from their serialized layout:

- `message::game_message::UpdateData` is a variable-length list read until EOF;
  ygopro's unique layout makes it impossible to map soundly in safe Rust.
- Any type that uses a `Vec`: in memory a `Vec` is a pointer plus a length and a
  capacity, while on the wire it is a contiguous byte stream. Examples:
  `SelectBattleCommand`.
- `TypeChange`: differs because of `Netplayer` change.
- `Replay`: boxed because it is too large to inline in the message enum.

The following types differ noticeably because of design issues in the ygopro crate:

- `Netplayer`, and consequently `CorePlayer`.

# Why [`binrw`](https://docs.rs/binrw) instead of [`serde`](https://docs.rs/serde)

serde's data model does not fit ygopro's data types well, while binrw offers more
freedom — e.g. compressed formats like `Replay` and streamed structs like the query
structs (`CardPosition`, `InfoLocation`).
