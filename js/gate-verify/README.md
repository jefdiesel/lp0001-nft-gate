# gate-verify (JS)

Checks an LP-0001 membership proof in application code. No transaction is sent and no
server of yours is in the path. It runs the same `check()` as the on-chain consumer
(`crates/gate_core`), pinned to it by `vectors/gate-vectors.json`.

## What it trusts — read this first

- **The LEZ RPC it reads, fully.** The answer (the slot's state, the chain time, and that the
  gate account runs the pinned immutable image) is whatever that RPC says. A lying RPC can show
  a fact that does not exist or hide a newer one. An LEZ block header has no state root
  (LEZ v0.4.0-rc3 `lez/common/src/block.rs:51-57`), so nothing here can check the RPC.
  Use the user's own node. Several independent RPCs reduce the trust; they do not remove it.
- **The LEZ indexer RPC it reads for `verifyOnchainUse`**, for the Act transaction's
  `Gated` event: which transaction gated, the acting account and the nonce in the event.
  The record's `program_account_id` is the emitting *account*, not the consumer program,
  so the lookup filters by tx hash and event selector and checks the submission id inside
  the event data. A lying indexer can substitute a different event; use your own node.
- **The sequencer**: it verified the proof, the commitment root and the nullifiers when it
  included the gate's transaction, and its clock is the time the window is judged by.
- RISC Zero receipts, the LEZ privacy circuit, SHA-256.

The receipt is not re-verified here. It is not load-bearing: the prover chooses the commitment
root, only the sequencer checks it is real, and no RPC answers "is this root known" or "is this
nullifier spent" (spec §3.2).

## What a verifier needs

| Input | Where it comes from |
|---|---|
| `gate` | your configuration (the pinned, immutable gate) — never the proof object |
| `nftProgram` | your configuration: the reference NFT program the gate is pinned to |
| `collection` | your configuration: the collection you gate on |
| `zoneId` | your configuration: the zone's channel id (`getChannelId`) |
| `callerIdentity` | how the wallet's provider names your app (the wallet puts its hash in the audience) |
| `purpose`, `nonce` | your own: a purpose string and 32 random bytes you issued for this session |
| `proof.slot` | the wallet's proof object `{ gate, slot, not_before, expires_at, tx_hash }` |
| chain state | the slot's gate actor state (`getAccountView`) and the latest block's timestamp (`getLastBlockId`, `getBlock`) |

There is no ownership set to read: nothing on chain commits to the holders of a collection.
The chain state a verifier reads is the slot.

## Use

```js
import { verifyOffchain, b58decode32, PLAIN } from '@lez-nft-gate/gate-verify';

const pinned = { gate, nftProgram, collection, zoneId };        // Uint8Array(32) each
const nonce = crypto.getRandomValues(new Uint8Array(32));        // issue, then ask the wallet
const v = await verifyOffchain({
  rpcUrl: 'http://127.0.0.1:3310',
  pinned,
  proof,                                                          // from the wallet
  use: { callerIdentity, purpose: new TextEncoder().encode('Check in'), nonce },
});
// v.name: OK | NO_FACT | MALFORMED | BINDING_MISMATCH | NOT_YET_VALID | EXPIRED | RPC_UNAVAILABLE
show(v.name === 'OK' ? 'Valid' : PLAIN[v.name], `read from ${v.rpc}, which is trusted for this answer`);
// then mark `nonce` used
```

Accept a fact only for a nonce you issued; re-read the same slot until expiry to see the fact
is still current. Do not hand out a reusable pass key.

## Re-checking an on-chain use

`verifyOnchainUse({ rpcUrl, eventsUrl, pinned, proof, consumer, submissionId, txHash, fetchFn })`
reads the Act transaction's `Gated` event over the LEZ indexer RPC and takes the acting
account and nonce **from that event**, not from its caller. `eventsUrl` defaults to
`rpcUrl`; use a node or proxy that serves both the indexer's `getEvents` and the
sequencer's `getAccountView`/`getLastBlockId`/`getBlock` methods, or pass the two URLs
separately. `txHash` is the 32-byte hash of the `Act` transaction. A transaction with no
`Gated` event for this submission id answers `NO_FACT`; an event for another pass answers
`BINDING_MISMATCH`. The result includes `gated: { actor, nonce, blockId }` as read.

## Outcome codes

`OK` 0, `NO_FACT` 1, `MALFORMED` 2, `BINDING_MISMATCH` 3, `NOT_YET_VALID` 4, `EXPIRED` 5,
`NOT_AUTHORIZED` 6 (consumer only), `RPC_UNAVAILABLE` 7 (library only), `NOT_A_MEMBER` 8
(wallet only). A non-member never reaches a verifier: the wallet sends nothing.
Wrong collection, wrong context and "a newer proof replaced it" share `BINDING_MISMATCH`:
the fact is opaque.

## Tests

`node --test test/*.test.mjs` — vectors, observed chain PDA, check cases, T10 consumer cases,
RPC flows against a fake RPC. `test/wasm.test.mjs` compares the wasm build with this code and
is skipped until `gate_wasm.wasm` is built (RUN-AFTER-3AM.txt).
