// SPDX-License-Identifier: MIT OR Apache-2.0
// The off-chain check over an LEZ JSON-RPC (spec §3.2). No transaction is sent.
//
// TRUST: this trusts the RPC it reads, fully (slot state, time, and that the gate account runs
// the pinned immutable image), and the sequencer (that it verified the proof when it included
// the transaction, and its clock). An LEZ block header has no state root, so nothing here can
// check the RPC's answer. Use your own node.

import {
  b58encode, b64decode, binding, check, contextOffchain, contextOnchain, readU64le, slotPda,
} from './core.mjs';

/** A JSON-RPC 2.0 call. `fetchFn` defaults to the global fetch. */
export async function rpcCall(url, method, params, fetchFn = globalThis.fetch) {
  const res = await fetchFn(url, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ jsonrpc: '2.0', id: 1, method, params }),
  });
  if (!res.ok) throw new Error(`${method}: HTTP ${res.status}`);
  const body = await res.json();
  if (body.error) throw new Error(`${method}: ${body.error.message ?? JSON.stringify(body.error)}`);
  return body.result;
}

/** getAccountView result → the actor state at `program` (empty when absent). */
export function actorStateFromAccount(result, program) {
  if (result === null || result === undefined) return new Uint8Array(0);
  const states = result?.data?.actor_states;
  if (!states || typeof states !== 'object') throw new Error('getAccountView: no data.actor_states');
  const bytes = states[b58encode(program)];
  if (!bytes) return new Uint8Array(0);
  if (!Array.isArray(bytes)) throw new Error('getAccountView: actor state is not a byte array');
  return Uint8Array.from(bytes);
}

/** A getBlock result is base64 of the block's borsh; the timestamp is header bytes 72..80. */
export function blockTimestamp(b64) {
  const b = b64decode(b64);
  if (b.length < 80) throw new Error('getBlock: block shorter than its header');
  return readU64le(b, 72);
}

/** Reads the slot's gate state and the latest block time. */
export async function readChain(url, gate, slot, fetchFn) {
  const pda = slotPda(gate, slot);
  const account = await rpcCall(url, 'getAccountView',
    [{ account_id: b58encode(pda), program_account_id: b58encode(gate) }], fetchFn);
  const state = actorStateFromAccount(account, gate);
  const last = await rpcCall(url, 'getLastBlockId', [], fetchFn);
  const block = await rpcCall(url, 'getBlock', [last], fetchFn);
  if (block === null) throw new Error(`getBlock(${last}): no block`);
  return { pda, state, now: blockTimestamp(block), blockId: last };
}

/**
 * Verify a proof object in app code.
 * pinned: { gate, nftProgram, collection, zoneId } (Uint8Array(32) each), the app's own config.
 * proof:  { gate, slot, ... } from the wallet; only `slot` (and that `gate` is the pinned one) is used.
 * use:    { callerIdentity, purpose, nonce } — the app's own values; the context is never
 *         taken from the proof object.
 * Returns { name, code, fact, now, pda, rpc, error }.
 */
export async function verifyOffchain({ rpcUrl, pinned, proof, use, fetchFn }) {
  const ctx = contextOffchain(pinned.zoneId, pinned.gate, use.callerIdentity, use.purpose, use.nonce);
  return verifyContext({ rpcUrl, pinned, proof, ctx, fetchFn });
}

/** Re-check an on-chain use in app code (actor and nonce read from the public Gated event). */
export async function verifyOnchainUse({ rpcUrl, pinned, proof, consumer, submissionId, actor, nonce, fetchFn }) {
  const ctx = contextOnchain(pinned.zoneId, pinned.gate, consumer, submissionId, actor, nonce);
  return verifyContext({ rpcUrl, pinned, proof, ctx, fetchFn });
}

export async function verifyContext({ rpcUrl, pinned, proof, ctx, fetchFn }) {
  const sameGate = proof.gate.length === 32 && proof.gate.every((x, i) => x === pinned.gate[i]);
  if (!sameGate) {
    return { name: 'BINDING_MISMATCH', code: 3, fact: null, rpc: rpcUrl, error: 'the proof names another gate than the pinned one' };
  }
  const expected = binding(pinned.nftProgram, pinned.collection, ctx);
  let chain;
  try {
    chain = await readChain(rpcUrl, pinned.gate, proof.slot, fetchFn);
  } catch (e) {
    return { name: 'RPC_UNAVAILABLE', code: 7, fact: null, rpc: rpcUrl, error: String(e?.message ?? e) };
  }
  const verdict = check(chain.state, expected, chain.now);
  return { ...verdict, now: chain.now, pda: chain.pda, blockId: chain.blockId, rpc: rpcUrl, error: null };
}
