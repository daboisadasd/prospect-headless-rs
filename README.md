# Prospect Headless (Rust) — milestone 0

This is the first implementation slice for replacing a The Cycle: Frontier UE4 `?listen` host with a standalone process.

The target is **protocol compatibility**, not embedding or executing cooked UE Blueprints in Rust. The long-term server will maintain authoritative gameplay state and emit the same UE replication/RPC state that the stock client expects.

## What is implemented now

- Dependency-free Rust UDP server/proxy.
- UE4 LSB-first bit writer/reader.
- UE packet trailing termination-bit handling.
- UE4-era `StatelessConnectHandlerComponent` 227-bit cookie handshake.
- HMAC-SHA1 cookie generation using two rotating 64-byte secrets.
- Extraction of the initial 14-bit server/client packet sequence seeds from the accepted cookie.
- Transparent UDP proxy mode to capture an exact known-good `?listen` session without modifying the client or host.
- Prospect/UE control-message IDs plus initial Hello/Challenge/Welcome payload codecs (not yet wired until packet+bunch framing lands).
- Deterministic HMAC/handshake regression vector in `cargo test`.
- Line-oriented wire log for deterministic offline protocol analysis.

The standalone mode deliberately stops after the connectionless handshake. The first post-handshake packet is logged; the next implementation milestone is the NetConnection packet header, control bunch parsing, and `NMT_Hello -> NMT_Challenge -> NMT_Login -> NMT_Welcome -> NMT_Join` flow.

## Why this architecture

The deiteris/Prospect tree already has a C# `Prospect.Server.Game` and a large `Prospect.Unreal` implementation. It includes the UE stateless handshake, channels, control messages, package map/NetGUID machinery, and an unfinished world model. Its `UNetDriver.TickFlush()` explicitly leaves `ServerReplicateActors` as TODO. That is the critical gap a real standalone game server must close.

Your UE4SS listen-host patch also gives us a behavioral checklist for the standalone server: authoritative player movement publication, projectile ownership/damage behavior, remote-player evac registration, interaction success, and host-only audio behavior.

## Build

Windows (Rust stable):

```powershell
cargo build --release
```

No third-party crates are required. Windows entropy uses `BCryptGenRandom`; Unix uses `/dev/urandom`. Run `BUILD_WINDOWS.ps1` to execute the protocol regression tests before producing the release executable.

### Continuous build verification

Every push to `master` runs `.github/workflows/build.yml` on GitHub Actions. The gate currently:

- runs `cargo check --all-targets` on Windows and Linux,
- runs all Rust tests on Windows and Linux,
- builds the release server on Windows and Linux,
- compiles and runs the independent C++ `NMT_Join` oracle,
- uploads `prospect-headless.exe` as the `prospect-headless-windows-x64` workflow artifact.

A revision should not be treated as build-verified until this workflow is green.

## Best first test: proxy the known-good listen host

Keep the existing `?listen` game host on its normal port, then run this proxy on a different port:

```powershell
.\target\release\prospect-headless.exe `
  --mode proxy `
  --bind 0.0.0.0:7788 `
  --upstream 127.0.0.1:7777 `
  --capture known-good-session.log
```

Point a client at:

```text
127.0.0.1:7788
```

Then perform one short deterministic session:

1. Connect and spawn.
2. Stand still for ~5 seconds.
3. Walk forward, strafe, jump, crouch/sprint.
4. Fire one hitscan weapon and one projectile weapon if available.
5. Pick up one item and interact with one container/object.
6. Aggro one creature and let it move/attack.
7. Enter/leave an evac volume and, if practical, complete an evac.
8. Disconnect normally.

The capture records every UDP datagram in both directions while the real UE listen server remains the authority. This becomes the wire-level oracle for the Rust replacement.

## Standalone handshake test

Stop anything already bound to the test port, then:

```powershell
.\target\release\prospect-headless.exe `
  --mode handshake `
  --bind 0.0.0.0:7777 `
  --capture standalone-handshake.log
```

Point the stock client to it. A successful milestone-0 test looks like:

```text
[handshake] challenge -> 127.0.0.1:xxxxx
[handshake] ESTABLISHED 127.0.0.1:xxxxx; server_seq=... client_seq=...
[handshake] post-handshake data ... Next milestone is packet/bunch + NMT_Hello.
```

The client is expected to stall/disconnect after that because control-channel handling is intentionally not implemented in this slice.

## Planned server layers

1. **Wire / connection layer** — stateless handshake, packet notify/reliability, bunches, channels.
2. **Control channel** — Hello/Challenge/Login/Welcome/Netspeed/Join and exact network-version handling.
3. **NetGUID/package map** — stable class/object path IDs expected by this exact game build.
4. **Actor replication** — spawn/despawn, FRepMovement, RepLayout fields, subobjects, RPC dispatch.
5. **Prospect gameplay model** — PlayerController/PlayerState/character, inventory, health/stamina, weapons/projectiles, interactables, evac.
6. **World data** — extract static spawn tables and relevant Blueprint defaults from cooked assets; Rust never needs to execute Blueprint VM bytecode if we reproduce their externally visible authoritative state transitions.
7. **AI** — initially server-side simplified state machines/nav data; then fidelity improvements where client behavior requires them.
8. **Persistence integration** — talk to the existing Prospect backend/database for loadout/deploy/evac results.

## Blueprint reality

Cooked Blueprints are not portable scripts that a Rust binary can simply load and execute. There are three practical choices:

- reimplement their authoritative behavior in Rust (preferred),
- extract Blueprint defaults/data tables and use them as data while replacing execution,
- or keep a stripped UE runtime as the server, which defeats much of the goal of a tiny standalone binary.

For preservation/server emulation, the first two approaches are realistic. Most client visuals, animation, audio and effects remain client-side; the server mainly needs to provide authoritative replicated properties, RPC results, movement, spawning, damage, inventory, AI decisions, and match state.

## License note

This prototype was informed by the AGPL-3.0-or-later deiteris/Prospect implementation and should be treated/distributed as AGPL-3.0-or-later unless it is later rewritten from independently documented wire observations.
