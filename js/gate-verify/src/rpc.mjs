// SPDX-License-Identifier: MIT OR Apache-2.0
// The off-chain check over an LEZ JSON-RPC (spec §3.2). No transaction is sent.
//
// TRUST: this trusts the RPC it reads, fully (slot state, time, and that the gate account runs
// the pinned immutable image), and the sequencer (that it verified the proof when it included
// the transaction, and its clock). An LEZ block header has no state root, so nothing here can
// check the RPC's answer. Use your own node.

import {
  b58encode, b64decode, binding, check, contextOffchain, contextOnchain, readU64le, sha256, slotPda, hex, CODES, unhex, decodeFact,
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

/**
 * Decode a public Act transaction from getTransaction's base64 LeeTransaction.
 * Layout (miniapp/web/logic.mjs:57-72, extended for Act's nonce): tag, actors,
 * authorized accounts, promotions, root actor + program, then the Act message
 * (tag 0, slot). Returns null for a different transaction shape.
 */
export function decodeActTransaction(b64) {
  const b = b64decode(b64);
  if (b[0] !== 0) return null;
  let at = 1;
  const u32 = (off) => new DataView(b.buffer, b.byteOffset + off, 4).getUint32(0, true);
  at += 4 + u32(at) * 64; // actors
  at += 4 + u32(at) * 32; // authorized accounts
  at += 4 + u32(at) * 8;  // promotions
  if (at + 64 > b.length) return null;
  const actor = b.slice(at, at + 32);
  const consumer = b.slice(at + 32, at + 64);
  at += 64;
  if (at + 4 > b.length) return null;
  const len = u32(at);
  const message = b.slice(at + 4, at + 4 + len);
  if (len !== 33 || message[0] !== 0) return null;
  return { actor, consumer, slot: message.slice(1, 33) };
}

/** Decode borsh ConsumerState { nonce: u64, pending: Option } for the state check. */
export function decodeConsumerState(bytes) {
  if (bytes.length < 9) return null;
  return { nonce: new DataView(bytes.buffer, bytes.byteOffset, 8).getBigUint64(0, true) };
}

/** The event's selector: sha256("lp0001-consumer::Gated")[..8] (LEZ convention). */
export function gatedSelector() {
  return sha256(new TextEncoder().encode('lp0001-consumer::Gated')).slice(0, 8);
}

const eq32 = (a, b) => a.length === 32 && b.length === 32 && a.every((x, i) => x === b[i]);

/**
 * Decode a Gated event's borsh data: submission id, slot, actor (32 bytes each),
 * then nonce and expires_at as little-endian u64 (crates/gate_interface/src/lib.rs:87-96).
 * Returns null unless the submission id is the one this consumer expects.
 */
export function decodeGatedData(data, submissionId) {
  if (data.length < 112 || !eq32(data.slice(0, 32), submissionId)) return null;
  return {
    slot: data.slice(32, 64),
    actor: data.slice(64, 96),
    nonce: readU64le(data, 96),
    expiresAt: readU64le(data, 104),
  };
}

/**
 * The Gated event for a transaction, read over the LEZ indexer RPC's getEvents
 * (a point query by tx hash, LEZ v0.4.0-rc3). The record's `program_account_id`
 * is the emitting program (LEZ sets it to actor.program_account_id), encoded as
 * a base58 AccountId. A record is accepted only if it is the pinned consumer
 * program; the submission id is checked inside the event data.
 * Returns { error } or { record: null } when nothing matches.
 */
export async function readGatedEvent({ rpcUrl, txHash, submissionId, programAccountId, fetchFn }) {
  const selector = hex(gatedSelector());
  const emitter = b58encode(programAccountId);
  let records;
  try {
    records = await rpcCall(rpcUrl, 'getEvents', [{ tx_hash: hex(txHash), selector, program_account_id: emitter }], fetchFn);
  } catch (e) {
    return { error: String(e?.message ?? e) };
  }
  if (!Array.isArray(records)) return { error: 'getEvents: not an array' };
  for (const record of records) {
    // Check every record: any program can emit the Gated selector, so the RPC
    // filter is only an optimisation. A forged event from another program cannot gate.
    if (record?.program_account_id !== emitter) continue;
    if (record?.tx_hash !== hex(txHash) || record.selector !== selector) continue;
    const gated = decodeGatedData(b64decode(record.data ?? ''), submissionId);
    if (gated) return { record, gated };
  }
  return { record: null };
}

/**
 * Re-check an on-chain use with no indexer (Job K). The Act transaction carries actor,
 * consumer, slot and the nonce it was made at; the consumer's current state (nonce bumped on
 * OK) shows it applied. `actTx` may be supplied already decoded, or is fetched and decoded
 * from getTransaction. `applied` is false for a reverted transaction; a caller with block
 * data should set it (this module cannot cheaply parse the block's reverted list).
 */
export async function verifyOnchainTx({
  rpcUrl, pinned, proof, submissionId, txHash, fetchFn, actTx, consumer,
}) {
  const bad = (name, error) => ({
    name, code: CODES.indexOf(name), fact: null, rpc: rpcUrl, error,
  });
  let act = actTx;
  if (!act) {
    try {
      const result = await rpcCall(rpcUrl, 'getTransaction', [hex(txHash)], fetchFn);
      if (!Array.isArray(result)) return bad('NO_FACT', 'getTransaction: not included');
      act = decodeActTransaction(result[0]);
      if (!act) return bad('BINDING_MISMATCH', 'not an Act transaction');
    } catch (e) {
      return bad('RPC_UNAVAILABLE', String(e?.message ?? e));
    }
  }
  if (act.applied === false) return bad('NO_FACT', 'the Act transaction reverted');
  if (!eq32(act.consumer, consumer)) return bad('BINDING_MISMATCH', "another consumer's Act");
  if (!eq32(act.slot, proof.slot)) return bad('BINDING_MISMATCH', 'the Act names a different pass');
  let state;
  try {
    const account = await rpcCall(rpcUrl, 'getAccountView', [
      { account_id: b58encode(act.actor), program_account_id: b58encode(consumer) },
    ], fetchFn);
    const key = b58encode(consumer);
    const bytes = account?.data?.actor_states?.[key];
    state = decodeConsumerState(Uint8Array.from(bytes ?? []));
    if (!state) return bad('MALFORMED', 'the consumer state does not decode');
  } catch (e) {
    return bad('RPC_UNAVAILABLE', String(e?.message ?? e));
  }
  const pda = slotPda(pinned.gate, proof.slot);
  let fact;
  try {
    const account = await rpcCall(rpcUrl, 'getAccountView', [
      { account_id: b58encode(pda), program_account_id: b58encode(pinned.gate) },
    ], fetchFn);
    const bytes = account?.data?.actor_states?.[b58encode(pinned.gate)];
    fact = decodeFact(Uint8Array.from(bytes ?? []));
    if (!fact) return bad('NO_FACT', 'the slot has no membership fact');
  } catch (e) {
    return bad('RPC_UNAVAILABLE', String(e?.message ?? e));
  }
  // Search downward from current-1 (the latest Act is the likely one), at most 1024 tries.
  let nonce = null;
  const first = state.nonce > 1024n ? state.nonce - 1024n : 0n;
  for (let n = state.nonce - 1n; n >= first; n--) {
    const ctx = contextOnchain(pinned.zoneId, pinned.gate, consumer, submissionId, act.actor, n);
    if (hex(binding(pinned.nftProgram, pinned.collection, ctx)) === hex(fact.binding)) { nonce = n; break; }
  }
  if (nonce === null) return bad('BINDING_MISMATCH', 'no matching nonce in the last 1024 Acts by this account');
  const ctx = contextOnchain(pinned.zoneId, pinned.gate, consumer, submissionId, act.actor, nonce);
  return verifyContext({ rpcUrl, pinned, proof, ctx, fetchFn });
}


/**
 * Re-check an on-chain use in app code. The acting account and nonce are READ
 * from the Act transaction's public Gated event over the LEZ indexer RPC; they
 * are no longer taken from the caller (Job F). `rpcUrl` serves the slot/time
 * reads (sequencer RPC); `eventsUrl` defaults to it and serves getEvents
 * (indexer RPC) — use a node or proxy that serves both if they differ.
 */
export async function verifyOnchainUse({ rpcUrl, eventsUrl, pinned, proof, consumer, submissionId, txHash, fetchFn, actTx }) {
  const eventUrl = eventsUrl ?? rpcUrl;
  const found = await readGatedEvent({ rpcUrl: eventUrl, txHash, submissionId, programAccountId: consumer, fetchFn });
  if ('error' in found) {
    return { name: 'RPC_UNAVAILABLE', code: 7, fact: null, rpc: eventUrl, error: found.error };
  }
  if (!found.record) {
    // Existing code NO_FACT: this transaction has no Gated event for the submission id.
    return {
      name: 'NO_FACT', code: 1, fact: null, rpc: eventUrl,
      error: 'no Gated event for this transaction and submission id',
    };
  }
  const { gated, record } = found;
  // If a getTransaction decoder is available too, the sources must agree.
  const tx = actTx ?? await (async () => {
    try {
      const result = await rpcCall(eventUrl, 'getTransaction', [hex(txHash)], fetchFn);
      return Array.isArray(result) ? decodeActTransaction(result[0]) : null;
    } catch { return null; }
  })();
  if (tx && (
    !eq32(tx.actor, gated.actor) || !eq32(tx.slot, gated.slot)
    || !eq32(tx.consumer, consumer)
  )) {
    return { name: 'BINDING_MISMATCH', code: 3, fact: null, rpc: eventUrl,
      error: 'the Gated event and the Act transaction disagree' };
  }
  if (!eq32(gated.slot, proof.slot)) {
    // Existing code BINDING_MISMATCH: the event is for a different pass.
    return {
      name: 'BINDING_MISMATCH', code: 3, fact: null, rpc: eventUrl,
      error: 'the Gated event names a different pass than the proof object',
    };
  }
  const ctx = contextOnchain(pinned.zoneId, pinned.gate, consumer, submissionId, gated.actor, gated.nonce);
  const verdict = await verifyContext({ rpcUrl, pinned, proof, ctx, fetchFn });
  return {
    ...verdict,
    gated: { actor: b58encode(gated.actor), nonce: gated.nonce, blockId: record.block_id },
  };
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
