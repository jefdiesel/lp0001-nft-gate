// SPDX-License-Identifier: MIT OR Apache-2.0
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createMembershipClient, validateRequest, sanitizeProof, toStatus, METHODS } from '../src/membership.mjs';

const COLL = '7yaKPERvgp931U3nPq5ZWnF82uVbfQo4DDH7spFGKCuY';
const NONCE = 'ab'.repeat(32);
const ok = (value) => JSON.stringify({ success: true, value, error: null });
const busy = () => JSON.stringify({ success: false, busy: true, value: null, error: 'The wallet is busy; still working.' });
const refused = (error) => JSON.stringify({ success: false, value: null, error });

/** A fake wallet module: scripted replies per method, records calls. */
function fakeWallet(script) {
  const calls = [];
  return {
    calls,
    async call(method, ...args) {
      calls.push([method, ...args]);
      const q = script[method];
      if (!q || q.length === 0) throw new Error(`unexpected ${method}`);
      return q.length > 1 ? q.shift() : q[0];
    },
  };
}

const opts = { backoffMs: 1, intervalMs: 1 };

test('a request carries no account, audience or context, and returns only a request id', async () => {
  const w = fakeWallet({ [METHODS.request]: [ok({ request: 'r1', state: 'pending', kind: 'membership_proof' })] });
  const mp = createMembershipClient(w, opts);
  const r = await mp.requestMembershipProof({ collection: COLL, purpose: 'Check in', nonce: NONCE, ttl: 3600000 });
  assert.deepEqual(r, { requestId: 'r1' });
  const sent = JSON.parse(w.calls[0][1]);
  assert.deepEqual(Object.keys(sent).sort(), ['collection', 'nonce', 'purpose', 'ttl']);
});

test('busy is retried; a refusal is final', async () => {
  const w = fakeWallet({ [METHODS.request]: [busy(), busy(), ok({ request: 'r2' })] });
  const mp = createMembershipClient(w, opts);
  assert.deepEqual(await mp.requestMembershipProof({ collection: COLL, purpose: 'x', nonce: NONCE, ttl: 1 }), { requestId: 'r2' });
  assert.equal(w.calls.length, 3);
  const w2 = fakeWallet({ [METHODS.request]: [refused('the membership-proof capability is not granted')] });
  await assert.rejects(createMembershipClient(w2, opts).requestMembershipProof({ collection: COLL, purpose: 'x', nonce: NONCE, ttl: 1 }), /not granted/);
  assert.equal(w2.calls.length, 1);
});

test('bad requests never reach the wallet', () => {
  const base = { collection: COLL, purpose: 'p', nonce: NONCE, ttl: 1000 };
  assert.throws(() => validateRequest({ ...base, collection: 'not-base58!' }), /collection/);
  assert.throws(() => validateRequest({ ...base, nonce: 'AB'.repeat(32) }), /nonce/);
  assert.throws(() => validateRequest({ ...base, nonce: 'ab' }), /nonce/);
  assert.throws(() => validateRequest({ ...base, ttl: 86400001 }), /ttl/);
  assert.throws(() => validateRequest({ ...base, purpose: '' }), /purpose/);
  assert.throws(() => validateRequest({ ...base, onchain: { consumer: COLL, actor: COLL } }), /acting account is chosen in the wallet/);
  assert.deepEqual(validateRequest({ ...base, onchain: { consumer: COLL } }).onchain, { consumer: COLL });
});

test('states map to the spec statuses', () => {
  assert.equal(toStatus({ state: 'pending' }).status, 'awaiting-approval');
  assert.equal(toStatus({ state: 'running', phase: 'simulate' }).status, 'checking');
  assert.deepEqual(toStatus({ state: 'running', phase: 'prove', elapsed_seconds: 42 }), { status: 'proving', elapsedSeconds: 42 });
  assert.equal(toStatus({ state: 'running', phase: 'submit' }).status, 'sending');
  assert.equal(toStatus({ state: 'running', phase: 'confirm' }).status, 'waiting-for-block');
  assert.deepEqual(toStatus({ state: 'failed', error_code: 'NOT_A_MEMBER' }).code, 'NOT_A_MEMBER');
  assert.equal(toStatus({ state: 'failed', error_code: 'something internal' }).code, 'UNEXPECTED_OUTCOME');
  assert.equal(toStatus({ state: 'rejected' }).code, 'REJECTED');
});

test('a ready proof keeps only the proof object; anything else is dropped and reported', () => {
  const s = toStatus({ state: 'done', proof: { gate: 'g', slot: 's', not_before: 1, expires_at: 2, tx_hash: 't', account: 'Private/xyz', nullifier: 'n' } });
  assert.deepEqual(s.proof, { gate: 'g', slot: 's', not_before: 1, expires_at: 2, tx_hash: 't' });
  assert.deepEqual(s.dropped.sort(), ['account', 'nullifier']);
  assert.deepEqual(sanitizeProof({ act_tx_hash: 'a' }).proof, { act_tx_hash: 'a' });
});

test('onProofStatus reports each change once and stops at ready', async () => {
  const w = fakeWallet({
    [METHODS.status]: [
      ok({ state: 'pending' }), ok({ state: 'pending' }),
      ok({ state: 'running', phase: 'prove', elapsed_seconds: 1 }),
      ok({ state: 'running', phase: 'confirm' }),
      ok({ state: 'done', proof: { gate: 'g', slot: 's', not_before: 1, expires_at: 2, tx_hash: 't' } }),
    ],
  });
  const seen = [];
  const mp = createMembershipClient(w, opts);
  await new Promise((resolve) => {
    mp.onProofStatus('r1', (s) => { seen.push(s.status); if (s.status === 'ready') resolve(); });
  });
  assert.deepEqual(seen, ['awaiting-approval', 'proving', 'waiting-for-block', 'ready']);
});

test('no status reads while the app is hidden', async () => {
  const w = fakeWallet({ [METHODS.status]: [ok({ state: 'pending' })] });
  const mp = createMembershipClient(w, { ...opts, isVisible: () => false });
  const stop = mp.onProofStatus('r1', () => {});
  await new Promise((r) => setTimeout(r, 20));
  stop();
  assert.equal(w.calls.length, 0);
});
