# LP-0001 integration guide: NFTs as a wallet feature

This is for an app that wants to **show, send, receive and prove** NFTs. LP-0001 is a wallet
feature, not a gating service: the gate proves membership without revealing which NFT or which
account. Every call below is read-only in the guide; nothing here deploys or sends a real
transaction. Sources are named at the end of each section.

All wallet calls use the Provider SDK envelope `{success, value, error, busy?}` and the QML bridge
`logos.callModuleAsync("names", method, args, cb)`. The bridge hands back **JSON text**, so an
app-side call must `JSON.parse` it before reading `.value`; `createMembershipClient` below does that
internally, so do not parse again for its calls. **Retry on `busy`** and poll a request by id;
a boolean `success` is not a result. (Source: `logos-names-wallet-1121/docs/wallet/PROVIDER-SDK.md`,
§"The reply envelope" and its QML example at line 722; reading the Names Wallet repo is read-only
here.)

## 0. Setup: one client and the pinned gate

```js
import { createMembershipClient } from '../../js/wallet-shim/src/membership.mjs';
import { verifyOffchain, PLAIN } from '../../js/gate-verify/src/core.mjs';

// The QML bridge: a module call, never a network fetch from the app.
const transport = {
  call: (method, ...args) => new Promise(ok =>
    logos.callModuleAsync('names', method, args, ok)),
};
const membership = createMembershipClient(transport, {
  retries: 6, backoffMs: 500, intervalMs: 5000,
});

// Your configuration, NOT the wallet's proof object.
const pinned = {
  gate: /* base58, immutable gate program */,
  nftProgram: /* base58, the reference NFT program */,
  collection: /* base58 collection id */,
  zoneId: /* getChannelId of the chain you use */,
};
```

Expected shape (from `vectors/gate-vectors.json`): the derivation vector `slot-0` gives
`pda_b58: "FzZC7FExfZSH5F6ngYR8aWVkRdxqBv8Bqb4tvdTzYwMc"`; the observed rc3 `pda_b58` is
`"2n2NPDMdqV6XDUwXaGnYcjKnUnsm5Dbx4ovUPhReRQxx"`. Source:
`js/gate-verify/README.md` and `vectors/gate-vectors.json`.

## 1. Show a user's NFTs

Ask once, then read the grant. Public accounts print as `Public/<id>`; private ones are
app-specific handles like `Private/~<40 hex>` — the app never sees another app's private account.

```js
const ask = JSON.parse(await transport.call('request_nft_access', 'Show your collectibles'));
// if (ask.busy) retry; if (!ask.success) show ask.error
const id = ask.value.request ?? ask.value;          // pending request
let granted = null;
while (!granted) {
  const r = JSON.parse(await transport.call('nft_request_status', id));
  if (r.busy) continue;
  if (!r.success) throw new Error(r.error);
  if (r.value.state === 'done') granted = JSON.parse(await transport.call('granted_nfts'));
  else if (['failed', 'rejected', 'expired', 'cancelled'].includes(r.value.state)) {
    throw new Error(`NFT access ${r.value.state}`);
  }
  await new Promise(ok => setTimeout(ok, 500));
}
const rows = granted.value.nfts.map(n =>
  `${n.collection}: ${n.print_balance} on ${n.account}`);
```

Expected output shape (fixture `logos-sdk-js/test/fixtures/network-rc3-published.json` has the
published program ids; the wallet's `granted_nfts` result shape is documented in PROVIDER-SDK.md
§`granted_nfts`): `["Door Pass: 1 on Public/5T7Az…"]` — one line per holding, never a private
key. To add Logos name deeds to the list:

```js
import { resolveName, readDeed } from 'logos-sdk-js/src/lns.js';
const record = await resolveName(client, LNS_PROGRAM, 'jef', Date.now());
if (record?.deed) {
  // resolveName already derived the accounts: record.deed is { definition, holding, nonce }.
  const held = await readDeed(client, record.deed); // { held, kind: 'master'|'copy'|null, definition }
}
```

Expected: `normalizeName('  @ALICE.lez ') === 'alice'` and
`publicView({account:'PublicAccount', texts:{forward:'bob'}}).publicAccount === 'PublicAccount'`
(`logos-sdk-js/test/fixtures/*`, asserted in `logos-sdk-js/test`). `readDeed` also returns the
decoded `definition`, so a definition mismatch is visible to the app. Source: PROVIDER-SDK.md
§`granted_nfts`; `logos-sdk-js/src/{lns,tx}.js`.

## 2. Send one NFT

Consent is a separate sheet; the wallet names the dApp, account, collection and asset. An app with
only `membership-proof` **cannot** send — it needs the `nft_transfers` grant.

```js
await transport.call('request_nft_transfers', 'Send a Door Pass to alice');
const destination = 'alice';                 // an LNS name, resolved by the wallet
const proposal = JSON.parse(await transport.call('propose_nft_transfer',
  nft.program, nft.account, destination,
  `Send 1 ${nft.collection} to alice`));
// poll JSON.parse(await transport.call('nft_request_status', proposal.value.request)) until done/failed
if (proposal.value.tx) show(`Sent. Transaction ${proposal.value.tx}`);
```

Expected output after `done`: a single `tx` hash and the line `Sent. Transaction <hash>`. The
wallet refuses another party's private receiving keys (`PROVIDER-SDK.md` §`propose_nft_transfer`);
do not ask for them. Source: PROVIDER-SDK.md §"Transfers"; `logos-sdk-js/src/tx.js` for the
underlying public/private send API.

## 3. Receive one

There is no "receive" provider call: the sender names your LNS name or `Public/<id>`. Your wallet
must first initialise a holding for that definition (Names Wallet Receive sheet, `InitializeHolding
{definition}`), otherwise the credit waits as a pending message until you accept it. After that,
`granted_nfts()` shows it. Source: `docs/wallet/NFT.md` §"Receive" (Names Wallet repo);
PROVIDER-SDK.md §`granted_nfts`.

```js
// No app-side call is needed to receive. Verify the result the same way as §1:
const r = JSON.parse(await transport.call('nft_request_status', requestId));
show(r.value.state === 'done' ? `Received: ${r.value.tx}` : `Not received: ${r.value.state}`);
```

Expected: `Received: <tx hash>` once the wallet accepts the pending credit; before that, the
status stays pending. Do not poll for someone else's private account. (The status reply is JSON
text too, hence the `JSON.parse` above.)

## 4. Claim an LNS name's NFT (deed)

**Not available through the Provider SDK.** It is a wallet action: "My names → Claim NFT (free)"
or the CLI `names deed claim -- <name>`; the on-chain message is `ClaimDeed {id, nonce, length}`.
The app may only read the result:

```js
const before = await resolveName(client, LNS_PROGRAM, 'jef', Date.now());
show(before?.deed ? 'This name already has a deed NFT.' : 'No deed yet: claim it in the wallet.');
```

Expected before a claim: `No deed yet: claim it in the wallet.` After the wallet claims,
`resolveName(...).deed` is `{ definition, holding, nonce }` (the accounts are derived inside
`resolveName`; fields `id` and `ownerTag` are only on the decoded name record, not on its result),
and `readDeed(client, record.deed)` returns `{ held: true, kind: 'master', definition }`. Source:
`docs/wallet/DEEDS.md` in the Names Wallet repo; `names deed claim` in its CLI;
`logos-sdk-js/src/lns.js`.

Do **not** promise a `propose_call` snippet for `ClaimDeed`: no message/actor vector for it is
documented, so the guide does not ship one.

## 5. Check ownership privately (the proof)

This is the one flow that reveals nothing but a pass id. Issue a fresh nonce, ask the wallet,
then verify locally with the same `check()` the on-chain consumer uses.

```js
const use = {
  callerIdentity: 'members_door_ui',            // how the wallet names your app
  purpose: new TextEncoder().encode('Check in'),
  nonce: new Uint8Array(32),                    // from crypto.getRandomValues
};
const proofRequest = await membership.requestMembershipProof({
  collection: pinned.collection,
  purpose: 'Check in this app',
  nonce: Buffer.from(use.nonce).toString('hex'), // 64 lower-case hex
  ttl: 3600000,
});
const status = await membership.status(proofRequest.requestId);
const v = await verifyOffchain({
  rpcUrl: 'http://127.0.0.1:3310', pinned,
  proof: status.proof, use,
});
show(v.name === 'OK' ? 'Valid' : PLAIN[v.name]);
```

Expected fixture outputs (`vectors/gate-vectors.json`): `verifyOffchain` builds the **off-chain**
context, so for the `offchain-1` use values (caller identity, purpose and app nonce in that vector)
the expected binding is `40d30a463123180a5450e676bb16d15bd2f8907866af9eb9b8bb3af426f59545` and a
gate fact with that binding inside its window verifies `OK`. The `ok-mid` case's binding
`f28a29f45e7959dbcea32b80af61c2f2de41f830bb2b068d334772bc53cd55e4` is the **on-chain** context
(from `onchain-nonce-0`): a generic `check(state, binding, now)` with that binding returns `OK` at
`1763500201000`, but `verifyOffchain` with the `offchain-1` use returns `BINDING_MISMATCH` against
that same state. Fact `fact-ttl0` is not_before `1763500200000`, expires_at `1763503800000`. The
mini-app's own test expects the screen line `Holder of Door Pass, until 2025-11-18T22:10:00Z.` and
that it does not reveal `Which NFT, which account.` (`miniapp/test/logic.test.mjs`).

The proof object is sanitised to `{gate, slot, not_before, expires_at, tx_hash}` (plus
`act_tx_hash` for on-chain use). The shim maps `pending → awaiting-approval`, `running+prove →
proving`, `done → ready` (`js/wallet-shim/test/membership.test.mjs`).

**On-chain use is not ready in this checkout.** The wallet refuses a non-null `onchain` field,
and with no gate pinned every real proof fails with `GATE_NOT_PINNED`
(`provider_proof.rs:19-20,170-171,364,426`). Treat the on-chain mini-app screen as a demo driven
by a pasted `gate-cli prove --out` object. Source: PROVIDER-SDK.md §"On-chain use" caveats;
`js/gate-verify/README.md`; `docs/WALLET-INTEGRATION.md`.

## What to distrust (from the code, not guessed)

- The LEZ RPC is trusted fully: LEZ rc3 block headers have no state root, so a verifier cannot
  check a lying RPC. Use the user's own node (`js/gate-verify/README.md`).
- The gate, NFT program, collection and zone id come from **your configuration**, never the proof.
- The audience is the wallet's name for your app, hashed; an app cannot choose it by argument
  (`docs/WALLET-INTEGRATION.md` §4).
- PROVIDER-SDK.md's version numbers (0.15.3 / "ask for >=0.16.0") are stale: this wallet checkout
  is 0.17.0 and the mini-app metadata asks for >=0.18.0. The method names and status codes are
  backed by `provider_nft.rs` / `provider_proof.rs`.
