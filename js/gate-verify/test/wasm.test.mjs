// SPDX-License-Identifier: MIT OR Apache-2.0
// Compares the wasm build (the Rust check()) with the JS implementation over every vector.
// Skipped until the wasm exists: build it with the command in RUN-AFTER-3AM.txt, which writes
// js/gate-verify/gate_wasm.wasm.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import * as g from '../src/core.mjs';
import { loadGateWasm } from '../src/wasm.mjs';

const WASM = new URL('../gate_wasm.wasm', import.meta.url);
const V = JSON.parse(readFileSync(new URL('../../../vectors/gate-vectors.json', import.meta.url)));
const u = (h) => g.unhex(h);

test('wasm check() agrees with JS on every vector', { skip: !existsSync(WASM) && 'gate_wasm.wasm not built yet (RUN-AFTER-3AM.txt)' }, async () => {
  const w = await loadGateWasm(readFileSync(WASM));
  for (const c of V.check_cases) {
    const now = c.now === null ? null : BigInt(c.now);
    assert.equal(g.CODES[w.check(u(c.state), u(c.expected_binding), now)], c.outcome, c.name);
    assert.equal(g.CODES[w.check(u(c.state), u(c.expected_binding), null)], c.onchain_outcome, c.name);
  }
  for (const d of V.derivations.filter((d) => d.name.startsWith('offchain-'))) {
    const ctx = w.contextOffchain(u(d.zone_id), u(d.gate), u(d.caller_identity), u(d.purpose), u(d.app_nonce));
    assert.equal(g.hex(ctx), d.context, d.name);
    assert.equal(g.hex(w.binding(u(d.nft_program), u(d.collection), ctx)), d.binding, d.name);
  }
  for (const d of V.derivations.filter((d) => d.name.startsWith('onchain-'))) {
    const ctx = w.contextOnchain(u(d.zone_id), u(d.gate), u(d.consumer), u(d.submission_id), u(d.actor), BigInt(d.nonce));
    assert.equal(g.hex(ctx), d.context, d.name);
  }
  for (const d of V.derivations.filter((d) => d.name.startsWith('slot-'))) {
    assert.equal(g.hex(w.slotPda(u(d.gate), u(d.slot))), d.pda, d.name);
  }
});
