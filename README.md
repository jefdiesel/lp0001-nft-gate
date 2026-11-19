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

Licence: MIT OR Apache-2.0.
