# lez-nft-gate

LP-0001 private NFT ownership proof on LEZ v0.4.0-rc3: prove "I hold an NFT of collection C"
without showing which NFT or which account, check it on LEZ (a program) and in app code (a
library), bound to one use and expiring. It reads Logos's reference NFT program
(`lez-programs` `refactor/lez-v0.4.x` `dea23c2`) through that program's own `ReadState`; it
writes no NFT state and is not an NFT program.

Build spec: `$SRC_ROOT/LP0001-OWNERSHIP-PROOF-SPEC.md`. Condensed, with limits L1–L10,
threats and what building showed: [docs/DESIGN.md](docs/DESIGN.md). Owner questions:
[docs/OWNER-DECISIONS.md](docs/OWNER-DECISIONS.md). Upstream asks (drafts, not posted):
[docs/UPSTREAM-ASKS.md](docs/UPSTREAM-ASKS.md). Wallet interface:
[docs/WALLET-INTEGRATION.md](docs/WALLET-INTEGRATION.md).

**Status (2026-11-19): nothing has run on a chain.** Native and JS tests pass; the guests are
not built, nothing is deployed, no real proof exists. The ordered commands are in
[RUN-AFTER-3AM.txt](RUN-AFTER-3AM.txt).

## Layout

| Path | What |
|---|---|
| `crates/gate_core` | no_std, no LEZ deps: fact layout, derivations, time rule, nine outcome codes, `check()` |
| `crates/gate_interface` | borsh messages, consumer state, `Gated` event |
| `crates/gate_program` | the gate (guest: `guests/gate`; TEST ONLY variant: `guests/gate-unsafe-test`) |
| `crates/consumer_program` | the reference consumer (guest: `guests/consumer`) |
| `crates/gate_verify` | app-code verifier, Rust |
| `crates/gate_wasm` | `check()` for wasm32, C ABI (not built yet) |
| `crates/gate_wallet_rules` | wallet pre-checks, fail-closed outcome check, error codes |
| `client/` | `gate-cli` on the stock LEZ wallet crate (own workspace) |
| `js/gate-verify` | app-code verifier, JS (trust statement in its README) |
| `js/wallet-shim` | the `membership-proof` capability, dApp side |
| `miniapp/` | Members' door: static page and Basecamp skeleton |
| `tests/onchain/` | setup, deploy and T0–T13 scripts for the build host |
| `tests-only/` | T4's attacker stand-in; never deployed beyond the throwaway chain |
| `tools/` | `gen_vectors.py` (golden vectors, std-lib Python), `b58.py`, `search_tx.py`, `linkability.py` |
| `vectors/gate-vectors.json` | the one shared vector file (Rust, JS, QML-safe JS, consumer) |

## Tests (laptop)

```sh
tests/onchain/T00-unit.sh      # vectors check, cargo test --workspace, node tests
```

What it trusts and does not: the off-chain check trusts the LEZ RPC it reads and the sequencer
(no state root in headers). See `js/gate-verify/README.md` and docs/DESIGN.md.

Checking an on-chain use reads the Act transaction's `Gated` event where an indexer serves it.
A standalone chain has no events RPC: the verifiers then read the Act transaction and the
consumer's nonce, searching the last 1024 Acts for the proof's nonce. That path shows an Act
happened and a pass is live, but not which of several Acts made the current pass. The same
plain refusal codes apply. See `js/gate-verify/README.md`.

## NFT media and metadata

An NFT's metadata comes from its definition on the reference NFT program: `name`, `standard`, a
`uri` (at most 256 bytes) and `creators`, stored inline by the program
(`names-wallet:docs/wallet/NFT.md:99-105`). The image is fetched from that `uri`, not by this gate;
a definition whose state does not decode is shown as unreadable, never hidden.

The wallet never opens the `uri` until the person ticks to allow it for that collection — the
"Show its image…" link — and stays fully usable with images off
(`names-wallet:docs/wallet/NFT.md:107-114`,
`names-wallet:basecamp/qml/Main.qml:10971-10976`). Logos Storage upload and download are not
used yet (`names-wallet:docs/wallet/NFT.md:114`).

Licence: MIT OR Apache-2.0.

## Logos Storage client (requirement 30)

`crates/storage_client` is a small, chain-free client for large NFT metadata/media, closing the
"No storage client here" gap in `docs/REQUIREMENTS-MAP.md` row 30. It speaks the Logos Storage REST
shape: `POST /api/storage/v1/data` uploads bytes and returns a CID, `GET
/api/storage/v1/data/{cid}` fetches them. `StorageClient::upload_file`, `fetch` and `verify_media`
let a caller publish an image, point an NFT definition's inline `uri`/`media` field at it, and
check on display that the fetched bytes still hash to the advertised CID (or `sha256` field).

The CID used in this skeleton is the local convention `b` + base32(sha2-256 multihash prefix ‖
sha256(bytes)); its exact encoding must be checked against a real Logos Storage endpoint before
production use. Tests run only against a local fake HTTP server on an ephemeral loopback port —
no chain, no real network, no proving:

```sh
the build wrapper cargo +1.98.1 test -p storage_client
```

