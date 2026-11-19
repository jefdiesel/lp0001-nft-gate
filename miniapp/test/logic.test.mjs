// SPDX-License-Identifier: MIT OR Apache-2.0
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { describeVerdict, progressLine, PendingGuard, gatedFromAct, bothWays, parseProofObject, newNonce } from '../web/logic.mjs';
import * as Q from '../basecamp/src/qml/logic.mjs';
import * as g from '../../js/gate-verify/src/core.mjs';

const V = JSON.parse(readFileSync(new URL('../../vectors/gate-vectors.json', import.meta.url)));

test('screen 2: valid, what it establishes, what it does not reveal, the RPC trusted', () => {
  const d = describeVerdict({ name: 'OK', fact: { expiresAt: 1763503800000n } }, { collectionName: 'Door Pass', rpc: 'http://127.0.0.1:3310' });
  assert.equal(d.headline, 'Valid');
  assert.equal(d.establishes, 'Holder of Door Pass, until 2025-11-18T22:10:00Z.');
  assert.equal(d.doesNotReveal, 'Which NFT, which account.');
  assert.match(d.trust, /127\.0\.0\.1:3310.*trusted/);
  const e = describeVerdict({ name: 'EXPIRED', fact: null }, { collectionName: 'X', rpc: 'r' });
  assert.equal(e.ok, false);
  assert.match(e.headline, /^EXPIRED: Expired/);
});

test('progress lines', () => {
  assert.equal(progressLine({ status: 'proving' }, 125), 'Proving (about 10 minutes)… 2:05');
  assert.equal(progressLine({ status: 'failed', plainMessage: 'No.' }), 'Not done: No.');
  assert.equal(progressLine({ status: 'whatever' }), 'Preparing…');
});

test('one pending request per acting account', () => {
  const p = new PendingGuard();
  assert.ok(p.take('A'));
  assert.ok(!p.take('A'));
  assert.ok(p.take('B'));
  p.release('A');
  assert.ok(p.take('A'));
});

function u32(n) { const b = new Uint8Array(4); new DataView(b.buffer).setUint32(0, n, true); return b; }

test('a gated action read from its Act transaction', () => {
  const actor = new Uint8Array(32).fill(0xa1);
  const consumer = new Uint8Array(32).fill(0xcc);
  const slot = new Uint8Array(32).fill(0x51);
  const tx = g.concat(Uint8Array.of(0), u32(2), new Uint8Array(128), u32(1), new Uint8Array(32), u32(0),
    actor, consumer, u32(33), Uint8Array.of(0), slot, new Uint8Array(40));
  const r = gatedFromAct([Buffer.from(tx).toString('base64'), 77]);
  assert.equal(r.actor, g.b58encode(actor));
  assert.equal(r.consumer, g.b58encode(consumer));
  assert.equal(r.slot, g.hex(slot));
  assert.equal(r.blockId, 77);
  assert.throws(() => gatedFromAct([Buffer.from(Uint8Array.of(1)).toString('base64'), 1]), /not a public/);
});

test('same proof, both ways', () => {
  assert.deepEqual(bothWays({ actApplied: true }, { name: 'OK' }), { onchain: 'OK', offchain: 'OK', agree: true });
  assert.equal(bothWays({ actApplied: false, consumerError: 'Guest panicked: consumer: BINDING_MISMATCH' }, { name: 'BINDING_MISMATCH' }).agree, true);
  assert.equal(bothWays({ actApplied: true }, { name: 'EXPIRED' }).agree, false);
});

test('proof objects and nonces', () => {
  const p = parseProofObject(JSON.stringify({ gate: '11111111111111111111111111111111', slot: 'ab'.repeat(32), not_before: 1 }));
  assert.equal(p.slot.length, 32);
  assert.throws(() => parseProofObject('{"gate":"x"}'), /proof object/);
  assert.match(newNonce((n) => new Uint8Array(n).fill(1)), /^(01){32}$/);
});

// The Basecamp skeleton's QML-safe logic (no BigInt, no TextEncoder, no crypto) against the
// same vector file and the JS package.
test('QML-safe logic: sha256, derivations and check() on every vector', () => {
  for (const n of [0, 1, 55, 56, 64, 200]) {
    const d = new Uint8Array(n).map((_, i) => (i * 31) & 255);
    assert.equal(Q.hex(Q.sha256([Array.from(d)])), g.hex(g.sha256(d)));
  }
  for (const c of V.check_cases) {
    const now = c.now === null ? null : Number(c.now);
    assert.equal(Q.check(Q.unhex(c.state), Q.unhex(c.expected_binding), now).name, c.outcome, c.name);
  }
  for (const d of V.derivations) {
    if (d.name.startsWith('offchain-')) {
      const ctx = Q.contextOffchain(Q.unhex(d.zone_id), Q.unhex(d.gate), Q.unhex(d.caller_identity), Q.unhex(d.purpose), Q.unhex(d.app_nonce));
      assert.equal(Q.hex(ctx), d.context, d.name);
      assert.equal(Q.hex(Q.binding(Q.unhex(d.nft_program), Q.unhex(d.collection), ctx)), d.binding, d.name);
    }
    if (d.name.startsWith('onchain-') && Number(d.nonce) < 2 ** 53) {
      const ctx = Q.contextOnchain(Q.unhex(d.zone_id), Q.unhex(d.gate), Q.unhex(d.consumer), Q.unhex(d.submission_id), Q.unhex(d.actor), Number(d.nonce));
      assert.equal(Q.hex(ctx), d.context, d.name);
    }
    if (d.name.startsWith('slot-')) {
      assert.equal(Q.b58encode(Q.slotPda(Q.unhex(d.gate), Q.slot(Q.unhex(d.secret)))), d.pda_b58, d.name);
    }
  }
  assert.equal(Q.hex(Q.utf8('Check in ✓')), g.hex(new TextEncoder().encode('Check in ✓')));
  assert.equal(Q.hex(Q.b58decode32(V.observed_pda.pda_b58)), V.observed_pda.pda);
});
