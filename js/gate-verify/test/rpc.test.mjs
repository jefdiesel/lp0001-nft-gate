// SPDX-License-Identifier: MIT OR Apache-2.0
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import * as g from '../src/core.mjs';
import { verifyOffchain, verifyOnchainUse, verifyOnchainTx, decodeActTransaction, actorStateFromAccount, blockTimestamp, gatedSelector } from '../src/rpc.mjs';

const T = (b) => new Uint8Array(32).fill(b);
const NB = 1763500200000n;
const pinned = { gate: T(0x6a), nftProgram: T(0xaa), collection: T(0xc0), zoneId: T(0x2e) };
const enc = new TextEncoder();
const use = { callerIdentity: enc.encode('basecamp:members-door'), purpose: enc.encode('Check in this app'), nonce: T(7) };
const SLOT = T(0x51);

function blockB64(id, ts) {
  const b = new Uint8Array(8 + 32 + 32 + 8 + 9);
  const dv = new DataView(b.buffer);
  dv.setBigUint64(0, BigInt(id), true);
  b.fill(0x11, 8, 40);
  b.fill(0x22, 40, 72);
  dv.setBigUint64(72, BigInt(ts), true);
  return Buffer.from(b).toString('base64');
}

/** A fake LEZ RPC: answers the methods the verifier uses, records the calls. */
function fakeRpc({ state, now, down = false, events = [] }) {
  const calls = [];
  const pda = g.slotPda(pinned.gate, SLOT);
  const fetchFn = async (url, init) => {
    const req = JSON.parse(init.body);
    calls.push(req.method);
    assert.equal(init.method, 'POST');
    if (down) throw new Error('connect ECONNREFUSED');
    let result;
    if (req.method === 'getAccountView') {
      assert.equal(req.params[0].program_account_id, g.b58encode(pinned.gate));
      result = req.params[0].account_id === g.b58encode(pda) && state
        ? { nonce: 0, data: { actor_states: { [g.b58encode(pinned.gate)]: Array.from(state) } } }
        : null;
    } else if (req.method === 'getLastBlockId') {
      result = 99;
    } else if (req.method === 'getBlock') {
      assert.deepEqual(req.params, [99]);
      result = blockB64(99, now);
    } else if (req.method === 'getEvents') {
      assert.equal(req.params[0].program_account_id, g.b58encode(T(0xcc)), 'the consumer is in the filter');
      result = events;
    } else {
      return { ok: true, json: async () => ({ jsonrpc: '2.0', id: 1, error: { code: -32601, message: 'no method' } }) };
    }
    return { ok: true, json: async () => ({ jsonrpc: '2.0', id: 1, result }) };
  };
  return { fetchFn, calls };
}

function goodFact(ctx = g.contextOffchain(pinned.zoneId, pinned.gate, use.callerIdentity, use.purpose, use.nonce)) {
  return g.encodeFact({ binding: g.binding(pinned.nftProgram, pinned.collection, ctx), notBefore: NB, expiresAt: NB + 3600000n });
}

const proof = { gate: pinned.gate, slot: SLOT, not_before: NB, expires_at: NB + 3600000n };
const TXHASH = T(0x42);

/** An indexer EventRecord: the LEZ getEvents reply (borsh Gated data, base64). */
function gatedRecord({ actor, nonce, submissionId, slot = SLOT, program = T(0xcc) }) {
  const data = new Uint8Array(112);
  const dv = new DataView(data.buffer);
  data.set(submissionId, 0);
  data.set(slot, 32);
  data.set(actor, 64);
  dv.setBigUint64(96, nonce, true);
  dv.setBigUint64(104, NB + 3600000n, true);
  return {
    block_id: 42,
    tx_index: 0,
    tx_hash: g.hex(TXHASH),
    // LEZ sets EventRecord.program_account_id to the emitting program, not the account.
    program_account_id: g.b58encode(program),
    selector: g.hex(gatedSelector()),
    data: Buffer.from(data).toString('base64'),
  };
}

test('OK, with only reads, and the RPC named', async () => {
  const { fetchFn, calls } = fakeRpc({ state: goodFact(), now: NB + 5n });
  const v = await verifyOffchain({ rpcUrl: 'http://127.0.0.1:3310', pinned, proof, use, fetchFn });
  assert.equal(v.name, 'OK');
  assert.equal(v.now, NB + 5n);
  assert.equal(v.rpc, 'http://127.0.0.1:3310');
  assert.deepEqual(calls, ['getAccountView', 'getLastBlockId', 'getBlock']);
});

test('expired, not yet valid, no fact, mismatch', async () => {
  let r = fakeRpc({ state: goodFact(), now: NB + 3600000n });
  assert.equal((await verifyOffchain({ rpcUrl: 'x', pinned, proof, use, fetchFn: r.fetchFn })).name, 'EXPIRED');
  r = fakeRpc({ state: goodFact(), now: NB - 1n });
  assert.equal((await verifyOffchain({ rpcUrl: 'x', pinned, proof, use, fetchFn: r.fetchFn })).name, 'NOT_YET_VALID');
  r = fakeRpc({ state: null, now: NB });
  assert.equal((await verifyOffchain({ rpcUrl: 'x', pinned, proof, use, fetchFn: r.fetchFn })).name, 'NO_FACT');
  r = fakeRpc({ state: goodFact(), now: NB });
  const other = { ...use, nonce: T(8) };
  assert.equal((await verifyOffchain({ rpcUrl: 'x', pinned, proof, use: other, fetchFn: r.fetchFn })).name, 'BINDING_MISMATCH');
});

test('RPC down is RPC_UNAVAILABLE, never OK', async () => {
  const r = fakeRpc({ state: goodFact(), now: NB, down: true });
  const v = await verifyOffchain({ rpcUrl: 'x', pinned, proof, use, fetchFn: r.fetchFn });
  assert.equal(v.name, 'RPC_UNAVAILABLE');
  assert.match(v.error, /ECONNREFUSED/);
});

test('another gate in the proof object is refused without reading', async () => {
  const r = fakeRpc({ state: goodFact(), now: NB });
  const v = await verifyOffchain({ rpcUrl: 'x', pinned, proof: { ...proof, gate: T(0x6b) }, use, fetchFn: r.fetchFn });
  assert.equal(v.name, 'BINDING_MISMATCH');
  assert.deepEqual(r.calls, []);
});

test('same proof, both ways: the event supplies the on-chain context', async () => {
  // Job F: actor and nonce are read from the Gated event, not from the caller.
  const consumer = T(0xcc); const sub = T(0x5b); const actor = T(0xa1);
  const ctx = g.contextOnchain(pinned.zoneId, pinned.gate, consumer, sub, actor, 0n);
  const base = { rpcUrl: 'x', pinned, proof, consumer, submissionId: sub, txHash: TXHASH };
  const good = fakeRpc({ state: goodFact(ctx), now: NB + 1n, events: [gatedRecord({ actor, nonce: 0n, submissionId: sub })] });
  const v = await verifyOnchainUse({ ...base, fetchFn: good.fetchFn });
  assert.equal(v.name, 'OK');
  assert.equal(v.gated.actor, g.b58encode(actor));
  assert.equal(v.gated.nonce, 0n);
  assert.ok(good.calls.includes('getEvents'), 'the event is read over the RPC');

  const stale = fakeRpc({ state: goodFact(ctx), now: NB + 1n, events: [gatedRecord({ actor, nonce: 1n, submissionId: sub })] });
  assert.equal((await verifyOnchainUse({ ...base, fetchFn: stale.fetchFn })).name, 'BINDING_MISMATCH');
});

test('verifyOnchainUse: a wrong actor or nonce in the Gated event is BINDING_MISMATCH', async () => {
  const consumer = T(0xcc); const sub = T(0x5b); const actor = T(0xa1);
  const ctx = g.contextOnchain(pinned.zoneId, pinned.gate, consumer, sub, actor, 0n);
  const base = { rpcUrl: 'x', pinned, proof, consumer, submissionId: sub, txHash: TXHASH };
  const wrongActor = fakeRpc({
    state: goodFact(ctx), now: NB + 1n,
    events: [gatedRecord({ actor: T(0xa2), nonce: 0n, submissionId: sub })],
  });
  assert.equal((await verifyOnchainUse({ ...base, fetchFn: wrongActor.fetchFn })).name, 'BINDING_MISMATCH');
  const wrongNonce = fakeRpc({
    state: goodFact(ctx), now: NB + 1n,
    events: [gatedRecord({ actor, nonce: 2n, submissionId: sub })],
  });
  assert.equal((await verifyOnchainUse({ ...base, fetchFn: wrongNonce.fetchFn })).name, 'BINDING_MISMATCH');
});

test('verifyOnchainUse refuses a Gated event emitted by another program', async () => {
  // Any program can emit the selector; a forged event must not count as our consumer's Act.
  const consumer = T(0xcc); const sub = T(0x5b); const actor = T(0xa1);
  const ctx = g.contextOnchain(pinned.zoneId, pinned.gate, consumer, sub, actor, 0n);
  const r = fakeRpc({
    state: goodFact(ctx), now: NB + 1n,
    events: [gatedRecord({ actor, nonce: 0n, submissionId: sub, program: T(0x99) })],
  });
  const v = await verifyOnchainUse({
    rpcUrl: 'x', pinned, proof, consumer, submissionId: sub, txHash: TXHASH, fetchFn: r.fetchFn,
  });
  assert.equal(v.name, 'NO_FACT'); // existing code: no event from this consumer for this submission
});

test('verifyOnchainUse skips a forged event and uses the real one after it', async () => {
  const consumer = T(0xcc); const sub = T(0x5b); const actor = T(0xa1);
  const ctx = g.contextOnchain(pinned.zoneId, pinned.gate, consumer, sub, actor, 0n);
  const r = fakeRpc({
    state: goodFact(ctx), now: NB + 1n,
    events: [
      gatedRecord({ actor: T(0xa2), nonce: 9n, submissionId: sub, program: T(0x99) }),
      gatedRecord({ actor, nonce: 0n, submissionId: sub }),
    ],
  });
  const v = await verifyOnchainUse({
    rpcUrl: 'x', pinned, proof, consumer, submissionId: sub, txHash: TXHASH, fetchFn: r.fetchFn,
  });
  assert.equal(v.name, 'OK');
  assert.equal(v.gated.actor, g.b58encode(actor));
  assert.equal(v.gated.nonce, 0n);
});

test('verifyOnchainUse refuses a missing Gated event and a wrong submission id', async () => {
  const consumer = T(0xcc); const sub = T(0x5b); const actor = T(0xa1);
  const ctx = g.contextOnchain(pinned.zoneId, pinned.gate, consumer, sub, actor, 0n);
  const base = { rpcUrl: 'x', pinned, proof, consumer, submissionId: sub, txHash: TXHASH };
  const none = fakeRpc({ state: goodFact(ctx), now: NB + 1n });
  const missing = await verifyOnchainUse({ ...base, fetchFn: none.fetchFn });
  assert.equal(missing.name, 'NO_FACT');
  assert.match(missing.error, /no Gated event/);
  const wrongSub = fakeRpc({
    state: goodFact(ctx), now: NB + 1n,
    events: [gatedRecord({ actor, nonce: 0n, submissionId: T(0x99) })],
  });
  // An event for another submission id is not an event for this one: NO_FACT.
  assert.equal((await verifyOnchainUse({ ...base, fetchFn: wrongSub.fetchFn })).name, 'NO_FACT');
});

test('shared Gated-event vector: JS agrees with the Rust verifier', async () => {
  const v = JSON.parse(readFileSync(new URL('../../../vectors/gated-event-vector.json', import.meta.url), 'utf8'));
  const h = (k) => g.unhex(v[k]);
  const pinnedV = { gate: h('gate'), nftProgram: h('nft_program'), collection: h('collection'), zoneId: h('zone_id') };
  const proofV = { gate: h('gate'), slot: h('slot'), not_before: 0n, expires_at: 0n };
  const r = fakeRpc({ state: Uint8Array.from(v.fact), now: BigInt(v.now_ms), events: [v.event] });
  const verdict = await verifyOnchainUse({
    rpcUrl: 'x', pinned: pinnedV, proof: proofV,
    consumer: h('consumer'), submissionId: h('submission_id'), txHash: h('tx_hash'), fetchFn: r.fetchFn,
  });
  assert.equal(verdict.name, v.expected);
});

test('JSON shapes: lee_core account JSON and borsh-base64 block', () => {
  const acct = JSON.parse('{"nonce":7,"data":{"actor_states":{"11111111111111111111111111111111":[9,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]}}}');
  assert.equal(actorStateFromAccount(acct, new Uint8Array(32))[0], 9);
  assert.equal(actorStateFromAccount(acct, T(1)).length, 0);
  assert.equal(actorStateFromAccount(null, T(1)).length, 0);
  assert.equal(blockTimestamp(blockB64(5, 1763500000123n)), 1763500000123n);
});

// ---- Job K: no-indexer verification from the Act transaction -------------------------------
function fakeTxRpc({ fact, now, consumerNonce }) {
  const fetchFn = async (url, init) => {
    const req = JSON.parse(init.body);
    let result;
    if (req.method === 'getAccountView') {
      const program = req.params[0].program_account_id;
      const states = program === g.b58encode(T(0xcc))
        ? { [program]: Array.from([...new DataView(new ArrayBuffer(8)).setBigUint64 ? [0] : []]) }
        : {};
      // Build the consumer state bytes: nonce LE + pending None, or the slot fact.
      const bytes = program === g.b58encode(T(0xcc))
        ? (() => { const b = new Uint8Array(9); new DataView(b.buffer).setBigUint64(0, BigInt(consumerNonce), true); return Array.from(b); })()
        : Array.from(fact);
      result = { nonce: 0, data: { actor_states: { [program]: bytes } } };
      void states;
    } else if (req.method === 'getLastBlockId') {
      result = 99;
    } else if (req.method === 'getBlock') {
      result = blockB64(99, now);
    } else if (req.method === 'getEvents') {
      result = [];
    } else {
      return { ok: true, json: async () => ({ jsonrpc: '2.0', id: 1, error: { code: -1, message: 'Method not found' } }) };
    }
    return { ok: true, json: async () => ({ jsonrpc: '2.0', id: 1, result }) };
  };
  return fetchFn;
}

function noIndexerSetup(nonce = 0) {
  const consumer = T(0xcc); const sub = T(0x5b); const actor = T(0xa1); const slot = SLOT;
  const ctx = g.contextOnchain(pinned.zoneId, pinned.gate, consumer, sub, actor, 0n);
  const fact = g.encodeFact({ binding: g.binding(pinned.nftProgram, pinned.collection, ctx), notBefore: NB, expiresAt: NB + 3600000n });
  const txHash = T(0x42);
  const actTx = { actor, consumer, slot, applied: true, blockId: 99 };
  const base = { rpcUrl: 'x', pinned, proof, submissionId: sub, txHash, consumer, fetchFn: fakeTxRpc({ fact, now: NB + 5n, consumerNonce: nonce }) };
  return { base, actTx, sub, actor, slot };
}

test('no indexer: an applied Act and its nonce bump verify', async () => {
  const { base, actTx } = noIndexerSetup(1);
  const v = await verifyOnchainTx({ ...base, actTx });
  assert.equal(v.name, 'OK');
});

test('no indexer: a later second Act still verifies the first transaction', async () => {
  const { base, actTx } = noIndexerSetup(5); // A has acted many times since
  const v = await verifyOnchainTx({ ...base, actTx });
  assert.equal(v.name, 'OK');
});

test('no indexer: a reverted Act is refused', async () => {
  const { base, actTx } = noIndexerSetup(1);
  const v = await verifyOnchainTx({ ...base, actTx: { ...actTx, applied: false } });
  assert.equal(v.name, 'NO_FACT');
});

test('no indexer: another consumer\'s Act is refused', async () => {
  const { base, actTx } = noIndexerSetup(1);
  const v = await verifyOnchainTx({ ...base, actTx: { ...actTx, consumer: T(0x99) } });
  assert.equal(v.name, 'BINDING_MISMATCH');
});

test('both paths: an event that disagrees with the Act transaction is refused', async () => {
  const consumer = T(0xcc); const sub = T(0x5b); const actor = T(0xa1); const txHash = T(0x42);
  const ctx = g.contextOnchain(pinned.zoneId, pinned.gate, consumer, sub, actor, 0n);
  const r = fakeRpc({ state: goodFact(ctx), now: NB + 1n, events: [gatedRecord({ actor, nonce: 0n, submissionId: sub })] });
  const v = await verifyOnchainUse({
    rpcUrl: 'x', pinned, proof, consumer, submissionId: sub, txHash, fetchFn: r.fetchFn,
    actTx: { actor, consumer, slot: T(0x99), applied: true },
  });
  assert.equal(v.name, 'BINDING_MISMATCH');
});

test('decodeActTransaction reads actor, consumer and slot from a public tx', () => {
  const b = new Uint8Array(1 + 4 + 64 + 4 + 32 + 4 + 32 + 4 + 33);
  const dv = new DataView(b.buffer);
  let at = 0;
  b[at++] = 0;
  dv.setUint32(at, 0, true); at += 4; // actors
  dv.setUint32(at, 0, true); at += 4; // authorized
  dv.setUint32(at, 0, true); at += 4; // promotions
  b.set(T(0xa1), at); at += 32;
  b.set(T(0xcc), at); at += 32;
  dv.setUint32(at, 33, true); at += 4;
  b[at++] = 0;
  b.set(SLOT, at); at += 32;
  const decoded = decodeActTransaction(Buffer.from(b).toString('base64'));
  assert.equal(g.hex(decoded.slot), g.hex(SLOT));
  assert.equal(g.hex(decoded.actor), g.hex(T(0xa1)));
});

test('shared Act-transaction vector: JS agrees with the Rust no-indexer verifier', async () => {
  const v = JSON.parse(readFileSync(new URL('../../../vectors/act-tx-vector.json', import.meta.url), 'utf8'));
  const h = (k) => g.unhex(v[k]);
  const pinnedV = { gate: h('gate'), nftProgram: h('nft_program'), collection: h('collection'), zoneId: h('zone_id') };
  const proofV = { gate: h('gate'), slot: g.unhex(v.act.slot), not_before: 0n, expires_at: 0n };
  const state = Uint8Array.from(v.fact);
  const fetchFn = fakeTxRpc({ fact: state, now: BigInt(v.now_ms), consumerNonce: v.consumer_nonce });
  const verdict = await verifyOnchainTx({
    rpcUrl: 'x', pinned: pinnedV, proof: proofV,
    submissionId: h('submission_id'), txHash: h('tx_hash'),
    consumer: h('consumer'), fetchFn,
    actTx: {
      actor: g.unhex(v.act.actor), consumer: g.unhex(v.act.consumer), slot: g.unhex(v.act.slot),
      applied: v.act.applied,
    },
  });
  assert.equal(verdict.name, v.expected);
});

test('the bounded search finds a recent nonce and refuses a distant one', async () => {
  const consumer = T(0xcc); const sub = T(0x5b); const actor = T(0xa1); const txHash = T(0x42);
  const factFor = (n) => {
    const ctx = g.contextOnchain(pinned.zoneId, pinned.gate, consumer, sub, actor, n);
    return g.encodeFact({ binding: g.binding(pinned.nftProgram, pinned.collection, ctx), notBefore: NB, expiresAt: NB + 3600000n });
  };
  const base = {
    rpcUrl: 'x', pinned, proof, submissionId: sub, txHash, consumer,
    actTx: { actor, consumer, slot: SLOT, applied: true },
  };
  const recent = await verifyOnchainTx({
    ...base, fetchFn: fakeTxRpc({ fact: factFor(4990n), now: NB + 5n, consumerNonce: 5000 }),
  });
  assert.equal(recent.name, 'OK');
  const distant = await verifyOnchainTx({
    ...base, fetchFn: fakeTxRpc({ fact: factFor(3000n), now: NB + 5n, consumerNonce: 5000 }),
  });
  assert.equal(distant.name, 'BINDING_MISMATCH');
  assert.equal(distant.error, 'no matching nonce in the last 1024 Acts by this account');
});
