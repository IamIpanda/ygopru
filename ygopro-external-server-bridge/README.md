# ygopro-external-server-bridge

A bridge that runs an external ygopro binary as a duel engine.

This module spawns a ygopro binary as a subprocess, connects to it over TCP, and
proxies the client-to-server and server-to-client message streams. It implements
`RoomProvider` from [`ygopro-handler`](https://docs.rs/ygopro-handler) so it can be
used as the duel backend.
