# ygopro-derive

Procedural macros for the ygopro ecosystem, powering `ygopro-data`,
`ygopro-handler` and `ygopro`.

Most of macros used by `srvpro` also here because I'm lazy.

The macros are grouped by what they do:

| group | macros | purpose |
| ----- | ------ | ------- |
| protocol data | `Message`, `GameMessage` | map structs to wire types; generate masking logic for game messages |
| plugin state | `Attachment`, `Configuration` | per-duel shared state and environment-driven plugin configuration |
| handling | `handler`, `before`, `after`, `command` | turn functions into message/command handlers |
| registration | `register_to` | put handlers into [`linkme`](https://docs.rs/linkme) distributed slices at link time |

# Notes

- Macros emit code that resolves against `ygopro_data` and
  `ygopro_handler`; using them outside the ygopro workspace requires both
  crates as dependencies.
- The generated `FromRequest` impls rely on raw-pointer casts that are
  sound only under `ygopro-handler`'s layout guarantees; see that crate's
  soundness section before taking `&mut` of the same state twice.
