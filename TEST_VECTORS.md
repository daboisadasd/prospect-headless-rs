# Protocol regression vectors

These vectors are deliberately deterministic and can be checked independently of a live game.

## Stateless-connect challenge vector

Inputs:

- secret: bytes `00 01 02 ... 3f` (64 bytes)
- client endpoint string: `127.0.0.1:54321`
- timestamp: IEEE-754 little-endian `12.5`
- secret id: `0`
- restart: `false`

Cookie input bytes (`double timestamp` + UE FString endpoint):

```text
0000000000002940100000003132372e302e302e313a353433323100
```

Expected HMAC-SHA1 cookie:

```text
996dba28fbfeddc79db26387e22b654eb561f1de
```

Expected challenge datagram including the UE termination bit/padding:

```text
0100000000004801ca6cd345d9f7ef3eee941d3b145f2973aa0d8bf70e
```

Expected sizes:

- meaningful handshake payload: 227 bits
- with termination bit and byte padding: 29 bytes

This vector is embedded as a Rust unit test in `src/handshake.rs`.
