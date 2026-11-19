// SPDX-License-Identifier: MIT OR Apache-2.0
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash, randomBytes } from 'node:crypto';
import { readFileSync } from 'node:fs';
import * as g from '../src/core.mjs';

const V = JSON.parse(readFileSync(new URL('../../../vectors/gate-vectors.json', import.meta.url)));
const u = (h) => g.unhex(h);

test('own sha256 equals node crypto', () => {
  for (let n = 0; n < 300; n += 7) {
    const data = randomBytes(n);
    assert.equal(g.hex(g.sha256(data)), createHash('sha256').update(data).digest('hex'));
  }
  assert.equal(g.hex(g.sha256(new Uint8Array(0))), createHash('sha256').digest('hex'));
});

test('constants match the vectors', () => {
  const c = V.constants;
  assert.equal(String(g.QUANT_MS), c.QUANT_MS);
  assert.deepEqual(g.TTL_MS.map(String), c.TTL_MS);
  assert.equal(g.hex(g.DOMAIN_SLOT), c.DOMAIN_SLOT);
  assert.equal(g.hex(g.DOMAIN_CTX), c.DOMAIN_CTX);
  assert.equal(g.hex(g.DOMAIN_BIND), c.DOMAIN_BIND);
  assert.equal(g.hex(g.LEE_PDA_PREFIX), c.LEE_PDA_PREFIX);
  assert.deepEqual(g.CODES, c.codes);
});

test('derivations match the vectors', () => {
  for (const d of V.derivations) {
    if (d.name.startsWith('slot-')) {
      const sl = g.slot(u(d.secret));
      assert.equal(g.hex(sl), d.slot, d.name);
      const pda = g.slotPda(u(d.gate), sl);
      assert.equal(g.hex(pda), d.pda, d.name);
      assert.equal(g.b58encode(pda), d.pda_b58, d.name);
      assert.equal(g.hex(g.b58decode32(d.pda_b58)), d.pda, d.name);
    } else if (d.name.startsWith('onchain-')) {
      const nonce = BigInt(d.nonce);
      assert.equal(g.hex(g.payloadOnchain(u(d.submission_id), u(d.actor), nonce)), d.payload, d.name);
      const ctx = g.contextOnchain(u(d.zone_id), u(d.gate), u(d.consumer), u(d.submission_id), u(d.actor), nonce);
      assert.equal(g.hex(ctx), d.context, d.name);
      assert.equal(g.hex(g.binding(u(d.nft_program), u(d.collection), ctx)), d.binding, d.name);
    } else {
      const id = u(d.caller_identity);
      const purpose = u(d.purpose);
      assert.equal(g.hex(g.audienceOffchain(id)), d.audience, d.name);
      assert.equal(g.hex(g.payloadOffchain(purpose, u(d.app_nonce))), d.payload, d.name);
      const ctx = g.contextOffchain(u(d.zone_id), u(d.gate), id, purpose, u(d.app_nonce));
      assert.equal(g.hex(ctx), d.context, d.name);
      assert.equal(g.hex(g.binding(u(d.nft_program), u(d.collection), ctx)), d.binding, d.name);
    }
  }
});

test('the PDA observed on the rc3 chain', () => {
  const o = V.observed_pda;
  assert.equal(g.hex(g.b58decode32(o.program_b58)), o.program);
  const pda = g.slotPda(u(o.program), u(o.seed));
  assert.equal(g.b58encode(pda), o.pda_b58);
});

test('fact encodings match the vectors', () => {
  for (const s of V.states.filter((s) => s.kind === 'fact')) {
    const bytes = g.encodeFact({ binding: u(s.binding), notBefore: BigInt(s.not_before), expiresAt: BigInt(s.expires_at) });
    assert.equal(g.hex(bytes), s.bytes, s.name);
    const f = g.decodeFact(u(s.bytes));
    assert.equal(String(f.notBefore), s.not_before);
    assert.equal(String(f.expiresAt), s.expires_at);
  }
  for (const s of V.states.filter((s) => s.kind === 'holder')) {
    assert.equal(g.decodeFact(u(s.bytes)), null, s.name);
  }
});

test('every check case: off chain and on chain', () => {
  assert.equal(V.check_cases.length, 26);
  for (const c of V.check_cases) {
    const now = c.now === null ? null : BigInt(c.now);
    assert.equal(g.check(u(c.state), u(c.expected_binding), now).name, c.outcome, c.name);
    assert.equal(g.check(u(c.state), u(c.expected_binding), null).name, c.onchain_outcome, `${c.name} on chain`);
  }
});

test('T10: the consumer cases agree (same file as the consumer test)', () => {
  for (const c of V.consumer_cases) {
    assert.equal(g.check(u(c.state), u(c.expected_binding), null).name, c.onchain_outcome, c.name);
  }
});

test('codes and plain texts', () => {
  for (const [i, name] of g.CODES.entries()) {
    assert.ok(g.PLAIN[name], name);
    if (i < 6) {
      // codes 0..5 can come out of check()
      assert.ok(['OK', 'NO_FACT', 'MALFORMED', 'BINDING_MISMATCH', 'NOT_YET_VALID', 'EXPIRED'].includes(name));
    }
  }
});

test('wallet time rule', () => {
  for (let i = 0; i < 1000; i++) {
    const now = BigInt(Math.floor(Math.random() * 2 ** 50));
    const nb = g.notBeforeFor(now);
    assert.ok(nb <= now && now - nb < g.QUANT_MS && nb % g.QUANT_MS === 0n);
  }
  assert.equal(g.ttlClassFor(1), 0);
  assert.equal(g.ttlClassFor(3600001), 1);
  assert.equal(g.ttlClassFor(86400001), null);
});

test('random bytes are never OK', () => {
  for (let i = 0; i < 2000; i++) {
    const b = randomBytes(1 + Math.floor(Math.random() * 100));
    const r = g.check(b, randomBytes(32), BigInt(Date.now()));
    assert.ok(r.name === 'MALFORMED' || r.name === 'BINDING_MISMATCH', r.name);
  }
});
