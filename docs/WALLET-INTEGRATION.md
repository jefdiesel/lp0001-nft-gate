# Wallet integration: the `membership-proof` capability

For the wallet worker (Names Wallet, branch `names-nft` in their own repository: this repo does
not edit it). This is the interface they should implement in
the `names` module, in the style of their Provider SDK (`docs/wallet/PROVIDER-SDK.md` there:
snake_case methods, the `{success, value, error, busy?}` envelope, requests polled by id).
Spec: `LP0001-OWNERSHIP-PROOF-SPEC.md` §4. Names are proposals (the spec says so).

The dApp side is written and tested here: `js/wallet-shim` (`createMembershipClient`), 7 node
tests against a fake module.

## 1. A capability of its own

- Grant: `membership-proof`, separate from `request_access` and from `request_nft_access`.
  Neither of those gives it, and it gives neither of them.
- Permissions list wording: **"May ask you to prove you hold an NFT of a collection. Learns a
  pass id, nothing about your accounts."** Revocable per dApp.
- Asking for a proof needs no account access. An app with only this grant cannot learn an
  account, a balance, a holding or the token, cannot enumerate accounts, and cannot propose
  transactions (prize line 43).

## 2. Module methods (for any app)

| Method | What it does | Value |
|---|---|---|
| `request_membership_proof(request_json)` | Queues a proof request. `request_json`: `{collection, purpose, nonce, ttl, onchain?: {consumer}}` — collection base58, purpose 1–280 chars, nonce 64 lower-case hex (the app's 32 random bytes), ttl ms ≤ 86,400,000. Unknown fields are refused. There is no field for an account, an audience or a context. | a request (`kind: "membership_proof"`) |
| `membership_proof_status(request_id)` | How it stands. Only the app that asked can read it. | a request |
| `cancel_membership_proof(request_id)` | Withdraws it while pending, or while proving (cancel is allowed until sending). | a request, `state: "cancelled"` |

A request record:

```json
{ "request": "<32 hex>", "kind": "membership_proof",
  "state": "pending | running | done | failed | rejected | expired | cancelled",
  "phase": "check | simulate | prove | submit | act | confirm",   // while running
  "elapsed_seconds": 312,                                         // while proving
  "proof": { "gate": "<base58>", "slot": "<64 hex>", "not_before": 1763500200000,
             "expires_at": 1763503800000, "tx_hash": "<64 hex>", "act_tx_hash": "<64 hex>" },
  "error_code": "NOT_A_MEMBER | FEE_RESERVE | ACCOUNT_BUSY | OUT_OF_WINDOW | PROVER_OUT_OF_MEMORY | RPC_UNREACHABLE | GATE_NOT_PINNED | UNEXPECTED_OUTCOME",
  "error": "plain words, never the wallet's own text" }
```

`proof` is present only when `done`; `act_tx_hash` only for an on-chain use. **Nothing returns
an account id, a balance, a holding, an alias, a commitment or a nullifier.** The shim drops
any other field and reports it (`sanitizeProof`); a wallet must never send one.

The shim maps states to the spec's statuses: `pending` → `awaiting-approval`; `running` with
`check|simulate` → `checking`, `prove` → `proving {elapsedSeconds}`, `submit|act` →
`sending`, `confirm` → `waiting-for-block`; `done` → `ready {proof}`; the rest → `failed
{code, plainMessage}`.

## 3. Names app only

`pending_proof_requests()`, `approve_proof_request(id, holding_account, fee_payer, digest)`,
`reject_proof_request(id)`, `proof_grants()` → `[{app, since, capability:
"membership-proof", discloses: "a pass id, nothing about your accounts"}]`,
`revoke_proof_capability(app)`. For an on-chain request the approval also names the acting
public account (spec §4.1, owner question Q3 default: picked by the user inside the wallet).

## 4. The audience: filled by the wallet

`audience = sha256(UTF-8 caller identity)` for an off-chain request (gate_core
`audience_offchain`). The caller identity is the name Basecamp gives the calling module's
credential, read from the call itself (`current_caller()`, as the Provider SDK already does for
`request_access` and `app_secret`), never an argument. A dApp verifying off chain must use
that same name as `callerIdentity`. UNVERIFIED (spec §2.4): that Basecamp's caller name cannot
be chosen by another app; the Provider SDK doc states it is the module's metadata name.

On-chain requests use the consumer's program id as the audience (the consumer pins it).

## 5. Approval sheet (spec §4.2, wording to use)

Shown before any proving, visibly different from the account-access sheet:

> **[App name] asks you to prove you hold an NFT of [collection name].**
>
> **This becomes public on the chain:**
> - That someone made a membership proof now. Not which collection, and not for which app.
> - A pass id. It stays the same for this NFT's account. Apps you show it to can tell it is
>   the same holder as before. They cannot tell who you are.
> - When the pass starts and ends: until [time].
> - *(on-chain use only)* Your account [name, short id] will be shown as having acted as a
>   member of [collection name].
>
> **This is not shown:** which NFT, which account holds it, your balances, your other NFTs.
>
> **Good to know:**
> - It takes about 10 minutes on this computer and uses most of its memory.
> - It replaces your earlier pass for this NFT [made for App X, if one is live].
> - This account cannot send anything until it is done.
> - About 5 million is held as a fee reserve and mostly returned.
>
> **Pay the fee from:** ( ) this NFT's account  ( ) [gas account]
>
> Program checks: gate [verified / not verified], NFT program [verified / not verified].
>
> [Prove] [Cancel]

- Collection name: from the definition's inline metadata.
- Two holdings of the collection: the user picks one; the dApp never sees the choice; never
  rotate holdings on its own (that mints extra slots).
- Verification status: the program registry (LP-0023).
- Help text: the RPC used for membership paths sees the holder's and payer's commitments and
  the caller's IP; prefer the user's own node (spec §4.6).
- "Pay the fee from" offers the gas account only once test T5 passes (owner question Q5).

## 6. What the wallet runs, in order (reuse `crates/gate_wallet_rules` and `client/`)

`client/src/main.rs` (`gate-cli prove`) is a working reference on the stock wallet crate.

1. Memory (`enough_memory`, 15 GiB) → `PROVER_OUT_OF_MEMORY`.
2. Gate pinned: header image id and `immutable` match the pinned values (`gate_is_pinned`) →
   `GATE_NOT_PINNED`.
3. Lock the holding from now until inclusion → `ACCOUNT_BUSY` for another send.
4. Own-note membership, then the wallet's dry run (simulation): a gate panic
   `gate: NOT_A_MEMBER` → `NOT_A_MEMBER`.
5. Payer covers the dry run's reserve alone (`covers_reserve`) → `FEE_RESERVE`.
6. **Outcome check, fail closed** (`outcome_check`). The stock wallet crate proves and sends
   in one call; the reference client wraps the sequencer client (`WalletSequencerClient`) and
   runs the check on the proven transaction inside `send_transaction`, before it is forwarded
   (see `client/src/guard.rs`). Any failure → `UNEXPECTED_OUTCOME`, nothing sent.
7. Window: `plan(chain_now, ttl)` → `not_before = floor(now, 10 min)`, smallest TTL class.
8. `slot_secret`: 32 random bytes on first use, kept only in the shielded gate state; re-read
   from the wallet's own note after a restore.
9. Default blinded presentation only; explicit private payer; `refund_to` unset; fund holdings
   only by sealed transfer; private holdings only; do not collect refunds automatically.
10. Errors from LEZ map with `classify(text)`.

Progress (spec §4.5): "Preparing…" at once, then one stage line with a timer: Checking your NFT →
Proving (about 10 minutes) → Sending → Waiting for the block → Ready. The dApp may close; the
wallet keeps going and shows the result on a Passes page. Closing the wallet cancels and says so.
No background polling, no silent auto-renew. Optional "Retire this pass" (a throwaway context at
the shortest TTL). CLI mirror: `names proof request | status | list`.

The wallet module must also ship an in-process executor for the simulation: the stock path
needs an `r0vm` binary a user will not have (F §0; upstream ask 6).

## 7. The dApp side (`js/wallet-shim`)

```js
import { createMembershipClient } from '@lez-nft-gate/wallet-shim';
import { verifyOffchain } from '@lez-nft-gate/gate-verify';

const transport = { call: (m, ...a) => new Promise((ok) => logos.callModuleAsync('names', m, a, ok)) };
const mp = createMembershipClient(transport, { isVisible: () => appIsInFront });
const nonce = randomHex32();
const { requestId } = await mp.requestMembershipProof({ collection, purpose: 'Check in', nonce, ttl: 3_600_000 });
const stop = mp.onProofStatus(requestId, async (s) => {
  show(s);                                         // every status, at once, in plain words
  if (s.status === 'ready') show(await verifyOffchain({ rpcUrl, pinned, proof: s.proof,
    use: { callerIdentity: MY_MODULE_NAME, purpose: enc('Check in'), nonce: hexBytes(nonce) } }));
});
```

Busy replies are retried with backoff (the Provider SDK rule: every app retries on busy).
Status is read only while the app is open and in front (default every 5 s, a local module
call, not a network poll).
