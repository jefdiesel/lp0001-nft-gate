# Design: the LP-0001 private ownership proof ("slot gate")

The build spec is `$SRC_ROOT/LP0001-OWNERSHIP-PROOF-SPEC.md` (2026-11-18). This is it
condensed, plus what building it showed. Labels as in the spec: OBSERVED (ran on a chain),
READ (source), UNVERIFIED (nobody has run it; the test that settles it is named).

## The shape

The reference NFT program (`lez-programs` `refactor/lez-v0.4.x` `dea23c2`) is used as it is. We
add two small programs: the **gate** and a **reference consumer**.

1. The holder sends one private transaction rooted on its own account's gate actor:
   `Prove { collection, context, not_before, ttl_class, slot_secret? }`.
2. The gate asserts the holder authorised it (`is_authorized`), keeps a pending record, and
   asks the NFT program's actor **on the same account** for its state with the NFT program's
   own `ReadState`. The read target and the NFT program are not parameters.
3. On the reply (accepted only from exactly `(own account, NFT_PROGRAM)`), the gate checks an
   owned `Copy` of the collection (or a `Master` with print balance ≥ 1, owner question Q2),
   and writes one public fact to the holder's **slot**: a public PDA keyed by
   `sha256("lp0001-gate/v1/slot" ‖ secret)`, the secret living only in the holder's shielded
   gate state.
4. The fact is `{binding, not_before, expires_at}` (50 bytes), with
   `binding = sha256("lp0001-gate/v1/bind" ‖ NFT program ‖ collection ‖ context)`. The
   transaction's timestamp window is `[not_before, expires_at)`, so the fact can only land
   while it is valid, and `expires_at` is at most 24 h after a grid-aligned `not_before`.
5. One holding account has one slot; each new proof overwrites it.
6. **On LEZ**, the consumer's `Act { slot }` (signed by acting account A) reads the slot through
   the gate's `ReadState`, rebuilds the context from its own constants, A and A's nonce, runs
   `check()`, emits `Gated { submission_id, slot, actor, nonce, expires_at }`, bumps the nonce
   and bounds its own window by the fact's. **In app code**, a library reads the slot and the
   latest block time over RPC and runs the same `check()`.

Context: `sha256("lp0001-gate/v1/ctx" ‖ kind ‖ zone_id ‖ gate ‖ audience ‖ payload)`; kind
`0x01` on chain (audience = consumer program id, payload over submission id, A, A's nonce) or
`0x02` off chain (audience = hash of the caller as the wallet sees it, payload over the app's
purpose and nonce). Outcome codes: `OK, NO_FACT, MALFORMED, BINDING_MISMATCH, NOT_YET_VALID,
EXPIRED, NOT_AUTHORIZED, RPC_UNAVAILABLE, NOT_A_MEMBER` (`gate_core::Outcome`).

## Where it lives

| Part | Path | Status |
|---|---|---|
| Shared core, `check()` | `crates/gate_core` (no_std, no LEZ deps) | written, tested |
| Wire types | `crates/gate_interface` | written, tested (via programs) |
| Gate program | `crates/gate_program`, guest `guests/gate` | logic tested natively; guest not built |
| Reference consumer | `crates/consumer_program`, guest `guests/consumer` | logic tested natively; guest not built |
| App-code verifier | `crates/gate_verify` (Rust), `js/gate-verify` (JS), `crates/gate_wasm` (wasm) | Rust and JS tested; wasm written, not built |
| Wallet rules | `crates/gate_wallet_rules` | tested |
| Client | `client/` (`gate-cli`) | type-checks; not run |
| dApp SDK shim | `js/wallet-shim` | tested |
| Mini-app | `miniapp/` | logic tested; not deployed |
| Chain tests T0–T13 | `tests/onchain/` | T0 run; T1–T13 written, not run |

## Limits, in plain words (spec L1–L10)

- **L1. Per account, not per token.** A printed copy has no token id, so "that same token" can
  only mean "that holding account".
- **L2. Selling does not cancel a pass.** After the NFT moves, the old pass stays valid until it
  expires. The new account gets its own pass. Only the short validity bounds this.
- **L3. One holder can serve many accounts.** A holder can make pass after pass for different
  acting accounts. 100 acting accounts do not prove 100 holders; distinct pass ids can be
  counted, but moving the NFT to a fresh account gives a fresh pass id.
- **L4. The pass id is a pseudonym.** All passes from one holding share one pass id, so two apps
  comparing notes can tell it is the same holder (not who). Owner question Q4.
- **L5. A handle, not proof bytes.** A LEZ program cannot verify a receipt handed to it; the
  consumer checks a fact the sequencer verified when it was written.
- **L6. App code trusts an RPC.** Block headers carry no state root, so an app must believe the
  RPC about the pass and the time. Use your own node.
- **L7. No ownership set.** Nothing on chain lists or commits to a collection's holders. The
  chain state a verifier reads is the pass.
- **L8. One live pass per holding.** Proving for app B cancels an unused pass for app A; an
  on-chain pass is wasted if the acting account's nonce moves before `Act` lands.
- **L9. Small crowds.** You hide among the collection's private holders only. A holder who
  received from a public sender was named by that delivery.
- **L10. Lending.** A holder can prove for someone else's request. Every anonymous credential
  has this.

## Threats and what stops each (spec §7)

| Threat | What stops it | Left over | Test |
|---|---|---|---|
| Passing without owning | The gate reads the holder's own NFT actor and asserts an owned copy | none known | T2; native conversation tests |
| Reading someone else's holding | A private read needs the holder's `nsk` and current state (OBSERVED, F A2.2); the gate asserts `is_authorized` at a root on the holder's own account | the assert is UNVERIFIED on chain | **T3** (native: `an_unauthorized_root_is_refused`) |
| A look-alike NFT program answering "owned" | `NFT_PROGRAM` is a gate constant; the reply must come from `(own account, NFT_PROGRAM)`; the consumer pins program and collection | none known | native: `a_look_alike_nft_program_reply_is_not_taken_as_the_reply`; property test |
| A hostile program acting with the holder's authority | Gate immutable and pinned, names no caller-chosen program; the wallet's fail-closed outcome check | the reference NFT program itself, if it can be updated (ask 5) | **T4**; `gate_wallet_rules` tests |
| No token id for copies | nothing | L1 | — |
| Pass kept after transfer | short TTLs; on-chain use is single-use (nonce); one live fact per account | L2 | T9 |
| Fresh-account reuse | TTL; each hop costs several private transactions; `Gated` carries the slot | L2, L3 | T9 |
| One holder, many acting accounts | each use is a new ~10-minute proof; distinct slots countable | L3 | — |
| Read collides with the holder's own send | both spend one nullifier; the wallet locks the account | a wasted 10 minutes | T13 |
| Replay across contexts | the binding | none known | property tests; T7, T8 |
| Someone else using a public fact | the context names A and its nonce; `Act` needs A's signature; off chain, the app's nonce | lending (L10) | T8; native consumer tests |
| A far-future expiry | the window's lower bound and the TTL menu | UNVERIFIED on chain | **T6** |
| Back-dating | an early `not_before` only shortens the life | none | T6 |
| A hostile consumer in an on-chain request | the wallet sends `Act` only to a registry-verified consumer and checks the outcome | a verified, malicious consumer inside those bounds | — |
| A lying RPC, off chain | nothing in app code; use your own node | L6 | — |
| A dApp asking for another dApp's context | the wallet fills the audience from the caller | caller identity UNVERIFIED | — |
| Cross-zone or post-reset reuse | the zone id in the context | none known | property test |
| Garbage written to a slot | only gate code writes gate state; `Publish` only from own program at the derived PDA | none known | native `publish_elsewhere_is_refused` |
| A program update that breaks the rules | immutable gate header | no fixes either | **T12** |
| Size and fee side channels | the payer always covers the reserve | first write vs rewrite visible | T1, T7 |

## Security assumptions to publish

Soundness of the LEZ privacy circuit and RISC Zero; SHA-256 preimage and collision resistance;
the wallet's random secret; an honest sequencer clock; the RPC, for off-chain checks; the gate
at its address is the pinned immutable image; the NFT program at its address is not replaced.
No trusted setup of ours.

## What building it showed (where the spec does not hold, or had to be filled in)

1. **A program can read the time.** The spec says "A program cannot read the time" (§2.7) and
   the consumer "compares no clock" (§3.1). LEZ v0.4.0-rc3 has a clock system program that
   answers `ReadTimestamp { granularity }` to any program with a sync reply
   (`lez/programs/clock/src/main.rs:42-55`); it holds the *previous* block's timestamp (ticked
   as the last transaction of every block, `lez/programs/clock/core/src/lib.rs:14-15`). The
   design is unchanged: the transaction window bounds the inclusion time itself, which is
   stronger than the previous block's time. An off-chain verifier could also read the clock
   account instead of `getBlock`.
2. **The fail-closed outcome check cannot run before proving with the stock wallet crate.**
   `send_proven` proves and sends in one call (`lez/wallet/src/lib.rs:1814-1900`); its only
   hook sees pre-states (`:1649`); simulation returns only cycles. The client therefore wraps
   the sequencer client (`WalletCore` is generic over `WalletSequencerClient`, `:302-366`) and
   checks the proven transaction in `send_transaction`: after proving, before sending. The
   holder's post-state comes from a clone of the wallet's key chain following the transaction
   (`UserKeyChain::follow_transaction`, `storage/key_chain.rs:487`). Nothing unexpected reaches
   the sequencer; the proving time is still spent. Ask 6(c).
3. **The `Gated` event cannot be read over the sequencer RPC.** No method returns events
   (`rpc/src/lib.rs:47-165`); events come from the indexer's replay
   (`lez/common/src/transaction.rs:137-148`). The client and mini-app show a gated action from
   its `Act` transaction instead (`getTransaction`: root actor = A, message = `Act{slot}`;
   reverted flag from the block), and monthly counts come from scanning blocks for non-reverted
   `Act`s to the consumer, or from an indexer.
4. **Immutability, help text vs code.** The wallet CLI calls `immutable` "not
   protocol-enforced" (`lez/wallet/src/cli/programs/program_loader.rs`, `UpdateHeader` doc),
   while the loader asserts `!old_header.immutable` (`program_loader/core/src/lib.rs:144-145`).
   T12 settles it.
5. **Caller identity.** The Names Wallet's Provider SDK names an app by `current_caller()`, the
   module's metadata name (its `docs/wallet/PROVIDER-SDK.md`, "How it works"); the audience is
   `sha256` of that name. Whether another module can take that name stays UNVERIFIED.
6. **Encodings the spec left open** are fixed and pinned by vectors (`docs/OWNER-DECISIONS.md`,
   last section). `MALFORMED` is defined strictly as "not bytes the gate can write".
7. **Deploy constants are build-time.** The gate's `NFT_PROGRAM` and the consumer's five
   constants are `LP0001_*` environment variables (64 hex) read by `option_env!`; a guest build
   without them refuses to compile, so a test value can never be deployed. The gate's own id is
   never a constant (it is `input.receiver.program_account_id`).
8. **Test-only code.** The unsafe test gate (feature `unsafe-test-gate`: caller-named NFT
   program, no authorisation assert) and the T4 hostile program exist only for T3's control and
   T4, are separate deployments, and cannot write the real gate's slots (`Publish` is accepted
   only from the gate's own program).
