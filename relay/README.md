# ScreenOverlayPhysics local relay

This standalone Rust service is a loopback networking slice for exercising the
host/guest room protocol before Railway deployment. It deliberately does not run
physics or interpret scene payloads.

Run it with:

```powershell
cargo run --manifest-path relay/Cargo.toml
```

It listens on `127.0.0.1:8787` by default. Override that with `RELAY_BIND`.

- `GET /health` reports service and protocol health.
- `GET /ws` upgrades to a WebSocket.
- The first WebSocket message is `room.hostHello` or `room.guestHello`, matching
  the protocol crate. `relay.createRoom` and `relay.joinRoom` remain accepted as
  harness-only aliases.
- Host scene frames broadcast to guests.
- Guest interaction frames and keyframe requests forward only to the host.
- Room capacity is eight participants including the host.
- Invite secrets are stored only as SHA-256 hashes after room creation.

This harness ends a room immediately when the host disconnects. Resume tokens,
the two-minute host grace period, invite rotation, and Railway configuration are
intentionally deferred to the room-registry milestone.

Run the unit and loopback integration suite with:

```powershell
cargo test --manifest-path relay/Cargo.toml
```

## Two-instance desktop check

For a host and guest on the same Windows PC, start this relay, then launch the
guest with alternate local ports so it does not collide with the host:

```powershell
$env:SCREEN_OVERLAY_ENGINE_IPC_ADDR = '127.0.0.1:47741'
$env:SCREEN_OVERLAY_WINDOW_IPC_ADDR = '127.0.0.1:47742'
$env:SCREEN_OVERLAY_ROOM_BRIDGE_ADDR = '127.0.0.1:47743'
```

The default host ports remain `47731`, `47732`, and `47733`. Both instances use
`ws://127.0.0.1:8787/ws` unless `SCREEN_OVERLAY_LOCAL_RELAY_URL` is supplied.
