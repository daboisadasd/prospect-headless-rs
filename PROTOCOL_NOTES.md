# Protocol findings for the supplied Prospect build

## Static executable observations

Supplied `Prospect-Win64-Shipping.exe`:

- PE32+ x86-64.
- Link timestamp: 2023-02-28 10:15:59 UTC.
- Internal source paths include `C:\depot\PROSPECT\Releases\R2.7.0\UE4\...`.
- The binary contains the explicit string `Requires UE4.27.1 ...`, strongly placing this build on the UE4.27 generation.
- Contains `UIpNetDriver`, `GameNetDriver`, `PacketHandler`, `StatelessConnectComponent`, and stock post-challenge accept logging.
- Contains `UYReplicationGraph` and `Using custom replication driver UYReplicationGraph`.
- Contains URL/config strings `UseReplicationGraph=`, `NetworkVersion`, and `?EncryptionToken=%s`.

These observations match the UE4SS script's comment that the shipping game supports selecting the stock net driver and uses a custom replication graph for `YPlayerCharacter`.

## Existing deiteris/Prospect server-emulation state

The repository already includes:

- `Prospect.Server.Game`
- `Prospect.Unreal`
- UE bit serialization and packet headers
- stateless connect handshake
- `UNetConnection` / `UIpNetDriver`
- Control, Voice and Actor channels
- NetGUID/package-map support
- control message definitions for Hello, Welcome, Challenge, Netspeed, Login, Join, etc.

The important blocker is explicit in `UNetDriver.TickFlush`: `ServerReplicateActors` is TODO. The current `Prospect.Server.Game` also uses a ThirdPerson example map/game mode placeholder rather than Prospect's cooked classes.

## UE4SS-derived behavioral requirements

The supplied host script establishes concrete listen-server incompatibilities that a headless server should avoid by design:

- `YPlayerCharacter` authority copies need movement to be published; the script updates `ReplicatedMovement` and calls `ForceNetUpdate()` on a ~60 Hz cadence.
- Listen-host projectiles can be created with `m_clientSideProjectile` even when owned by an authority actor; clearing it restores the normal authoritative impact path.
- Remote authority players need explicit registration with `AAM_Escape_BP_C:OnPlayerJoined` for evacuation logic.
- `ServerStartInteractionInternal` is repaired by calling `ClientInteractionSuccessful()` in host mode.
- Host controller audio-occlusion components are disabled because a listen-server-specific path is unstable.

These are not necessarily protocol requirements. They are symptoms of running a client shipping build as a listen server. A purpose-built headless authority should implement the intended server semantics directly.

## Next reverse-engineering targets

1. Capture a successful proxied connect through the Rust relay.
2. Confirm exact post-handshake packet header version and NMT_Hello fields for this build.
3. Extract the LocalNetworkVersion actually sent by the stock client.
4. Record Welcome map/game-mode strings emitted by the working listen host.
5. Build a per-class network schema for the minimum actor set required to spawn one player.
6. Add actors incrementally: GameState, PlayerState, PlayerController, YPlayerCharacter, movement component/subobjects.
7. Only after two clients can see/move each other, add inventory/combat/AI/loot/evac.
