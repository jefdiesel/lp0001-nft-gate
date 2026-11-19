// SPDX-License-Identifier: MIT OR Apache-2.0
// Members' door (working name): the mini-app's own logic, apart from the page so node tests
// run the same code the page does. Spec §5.

import { b58decode32, b58encode, b64decode, hex, PLAIN } from '../../js/gate-verify/src/core.mjs';

/** 32 random bytes as hex: the app's nonce for one session. */
export function newNonce(rand = (n) => crypto.getRandomValues(new Uint8Array(n))) {
  return hex(rand(32));
}

/** What screen 2 shows for a verdict: valid or the code, what it establishes, what it does not reveal, and the trust note. */
export function describeVerdict(verdict, { collectionName, rpc }) {
  const ok = verdict.name === 'OK';
  const until = verdict.fact ? new Date(Number(verdict.fact.expiresAt)).toISOString().replace('.000Z', 'Z') : null;
  return {
    ok,
    headline: ok ? 'Valid' : `${verdict.name}: ${PLAIN[verdict.name] ?? 'unknown outcome'}`,
    establishes: ok ? `Holder of ${collectionName}, until ${until}.` : 'Nothing: the proof was not accepted.',
    doesNotReveal: 'Which NFT, which account.',
    trust: `Read from ${rpc}. That RPC is trusted for this answer: block headers carry no state root, so it cannot be checked here.`,
  };
}

/** The progress line (spec §4.5): one stage and a timer. */
export function progressLine(status, elapsedSeconds = 0) {
  const t = `${Math.floor(elapsedSeconds / 60)}:${String(elapsedSeconds % 60).padStart(2, '0')}`;
  switch (status.status) {
    case 'awaiting-approval': return 'Waiting for you to approve in the wallet…';
    case 'checking': return `Checking your NFT… ${t}`;
    case 'proving': return `Proving (about 10 minutes)… ${t}`;
    case 'sending': return `Sending… ${t}`;
    case 'waiting-for-block': return `Waiting for the block… ${t}`;
    case 'ready': return 'Ready.';
    case 'failed': return `Not done: ${status.plainMessage}`;
    default: return 'Preparing…';
  }
}

/** One pending request per acting account (spec §5; limit L8). */
export class PendingGuard {
  constructor() { this.busy = new Set(); }
  take(key) { if (this.busy.has(key)) return false; this.busy.add(key); return true; }
  release(key) { this.busy.delete(key); }
}

function readU32(b, at) { return new DataView(b.buffer, b.byteOffset + at, 4).getUint32(0, true); }

/**
 * A gated action from its Act transaction (getTransaction result `[base64 borsh, block id]`).
 * The sequencer RPC returns no events (see docs/DESIGN.md), so the action is read from the
 * transaction: a public transaction is tag 0, then TransactionMessage { public_context
 * { actors: set<Actor 64>, authorized: set<id 32>, promotions: set<u64> }, execution
 * { root: { to: Actor 64, message: vec<u8> }, … } }. UNVERIFIED against a real transaction
 * until test T8 (compare with `gate-cli gated`).
 */
export function gatedFromAct(result) {
  if (!Array.isArray(result)) throw new Error('getTransaction: not included');
  const b = b64decode(result[0]);
  if (b[0] !== 0) throw new Error('not a public transaction');
  let at = 1;
  at += 4 + readU32(b, at) * 64; // actors
  at += 4 + readU32(b, at) * 32; // authorized accounts
  at += 4 + readU32(b, at) * 8; // promotions
  const actor = b.slice(at, at + 32);
  const program = b.slice(at + 32, at + 64);
  at += 64;
  const len = readU32(b, at);
  const message = b.slice(at + 4, at + 4 + len);
  if (message.length !== 33 || message[0] !== 0) throw new Error('not an Act');
  return { actor: b58encode(actor), consumer: b58encode(program), slot: hex(message.slice(1)), blockId: result[1] };
}

/** "Same proof, both ways" (spec §5): the consumer's result and the library's, side by side. */
export function bothWays({ actApplied, consumerError }, verdict) {
  const onchain = actApplied ? 'OK' : (consumerError?.match(/consumer: ([A-Z_]+)/)?.[1] ?? 'REFUSED');
  return { onchain, offchain: verdict.name, agree: onchain === verdict.name };
}

/** Parse a pasted proof object (from the wallet or `gate-cli prove`). */
export function parseProofObject(text) {
  const p = JSON.parse(text);
  if (typeof p.gate !== 'string' || typeof p.slot !== 'string' || !/^[0-9a-f]{64}$/.test(p.slot)) {
    throw new Error('a proof object has gate (base58) and slot (64 hex)');
  }
  return { gate: b58decode32(p.gate), slot: Uint8Array.from(p.slot.match(/../g).map((h) => parseInt(h, 16))), raw: p };
}
