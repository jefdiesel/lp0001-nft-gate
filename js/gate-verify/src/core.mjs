// SPDX-License-Identifier: MIT OR Apache-2.0
// gate_core in plain JavaScript: the same derivations, layout, codes and check() as the Rust
// crate (crates/gate_core), pinned to it by vectors/gate-vectors.json. Synchronous, no
// dependencies (its own SHA-256), so it runs the same in node and in a browser.

export const QUANT_MS = 600000n;
export const TTL_MS = [3600000n, 21600000n, 86400000n]; // OWNER DECISION Q1 defaults
export const MASTER_COUNTS = true; // OWNER DECISION Q2 default
const enc = new TextEncoder();
export const DOMAIN_SLOT = enc.encode('lp0001-gate/v1/slot');
export const DOMAIN_CTX = enc.encode('lp0001-gate/v1/ctx');
export const DOMAIN_BIND = enc.encode('lp0001-gate/v1/bind');
export const LEE_PDA_PREFIX = concat(enc.encode('/LEE/v0.2/AccountId/PDA/'), new Uint8Array(8));
export const KIND_ONCHAIN = 0x01;
export const KIND_OFFCHAIN = 0x02;
export const FACT_LEN = 50;

/** The nine outcome codes, in number order (crates/gate_core Outcome). */
export const CODES = ['OK', 'NO_FACT', 'MALFORMED', 'BINDING_MISMATCH', 'NOT_YET_VALID', 'EXPIRED',
  'NOT_AUTHORIZED', 'RPC_UNAVAILABLE', 'NOT_A_MEMBER'];

/** A plain sentence per code, for users. */
export const PLAIN = {
  OK: 'Valid: a current membership proof for this collection and this use.',
  NO_FACT: 'No proof: the pass id has no proof written to it.',
  MALFORMED: 'Not a proof: the data at this pass id is not a gate fact.',
  BINDING_MISMATCH: 'Not for this use: the proof is for another collection or use, or a newer proof replaced it.',
  NOT_YET_VALID: "Not valid yet: the proof's start time is still ahead.",
  EXPIRED: "Expired: the proof's validity window has ended.",
  NOT_AUTHORIZED: 'Not authorised: the acting account did not sign.',
  RPC_UNAVAILABLE: 'Could not read the chain: no answer from the RPC.',
  NOT_A_MEMBER: 'Not a member: the wallet found no NFT of this collection to prove with.',
};

export function concat(...parts) {
  const len = parts.reduce((n, p) => n + p.length, 0);
  const out = new Uint8Array(len);
  let at = 0;
  for (const p of parts) { out.set(p, at); at += p.length; }
  return out;
}

// ---- SHA-256 (FIPS 180-4), small and synchronous ----
const K = new Uint32Array([
  0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
  0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
  0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
  0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
  0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
  0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
  0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
  0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
]);

export function sha256(...parts) {
  const msg = concat(...parts);
  const bitLen = BigInt(msg.length) * 8n;
  const padded = new Uint8Array(Math.ceil((msg.length + 9) / 64) * 64);
  padded.set(msg);
  padded[msg.length] = 0x80;
  const dv = new DataView(padded.buffer);
  dv.setBigUint64(padded.length - 8, bitLen, false);
  const h = new Uint32Array([0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19]);
  const w = new Uint32Array(64);
  const rotr = (x, n) => (x >>> n) | (x << (32 - n));
  for (let off = 0; off < padded.length; off += 64) {
    for (let i = 0; i < 16; i++) w[i] = dv.getUint32(off + i * 4, false);
    for (let i = 16; i < 64; i++) {
      const s0 = rotr(w[i - 15], 7) ^ rotr(w[i - 15], 18) ^ (w[i - 15] >>> 3);
      const s1 = rotr(w[i - 2], 17) ^ rotr(w[i - 2], 19) ^ (w[i - 2] >>> 10);
      w[i] = (w[i - 16] + s0 + w[i - 7] + s1) >>> 0;
    }
    let [a, b, c, d, e, f, g, hh] = h;
    for (let i = 0; i < 64; i++) {
      const S1 = rotr(e, 6) ^ rotr(e, 11) ^ rotr(e, 25);
      const ch = (e & f) ^ (~e & g);
      const t1 = (hh + S1 + ch + K[i] + w[i]) >>> 0;
      const S0 = rotr(a, 2) ^ rotr(a, 13) ^ rotr(a, 22);
      const maj = (a & b) ^ (a & c) ^ (b & c);
      const t2 = (S0 + maj) >>> 0;
      hh = g; g = f; f = e; e = (d + t1) >>> 0; d = c; c = b; b = a; a = (t1 + t2) >>> 0;
    }
    h[0] = (h[0] + a) >>> 0; h[1] = (h[1] + b) >>> 0; h[2] = (h[2] + c) >>> 0; h[3] = (h[3] + d) >>> 0;
    h[4] = (h[4] + e) >>> 0; h[5] = (h[5] + f) >>> 0; h[6] = (h[6] + g) >>> 0; h[7] = (h[7] + hh) >>> 0;
  }
  const out = new Uint8Array(32);
  const odv = new DataView(out.buffer);
  for (let i = 0; i < 8; i++) odv.setUint32(i * 4, h[i], false);
  return out;
}

// ---- encodings ----
export function hex(b) { return Array.from(b, (x) => x.toString(16).padStart(2, '0')).join(''); }
export function unhex(s) {
  const t = s.trim().replace(/^0x/, '');
  if (t.length % 2 !== 0 || /[^0-9a-fA-F]/.test(t)) throw new Error('bad hex');
  const out = new Uint8Array(t.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(t.slice(2 * i, 2 * i + 2), 16);
  return out;
}
export function u64le(v) {
  const out = new Uint8Array(8);
  new DataView(out.buffer).setBigUint64(0, BigInt(v), true);
  return out;
}
export function readU64le(b, at) { return new DataView(b.buffer, b.byteOffset + at, 8).getBigUint64(0, true); }

const B58 = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';
/** Base58 (Bitcoin alphabet), as LEZ prints account ids. */
export function b58encode(b) {
  let n = 0n;
  for (const x of b) n = n * 256n + BigInt(x);
  let s = '';
  while (n > 0n) { s = B58[Number(n % 58n)] + s; n /= 58n; }
  for (const x of b) { if (x !== 0) break; s = '1' + s; }
  return s;
}
/** Base58 to 32 bytes (an account id). */
export function b58decode32(s) {
  let n = 0n;
  for (const c of s.trim()) {
    const d = B58.indexOf(c);
    if (d < 0) throw new Error(`bad base58 character ${c}`);
    n = n * 58n + BigInt(d);
  }
  if (n >= 1n << 256n) throw new Error('longer than 32 bytes');
  const out = new Uint8Array(32);
  for (let i = 31; i >= 0; i--) { out[i] = Number(n & 0xffn); n >>= 8n; }
  return out;
}
export function b64decode(s) {
  if (typeof atob === 'function') return Uint8Array.from(atob(s), (c) => c.charCodeAt(0));
  return new Uint8Array(Buffer.from(s, 'base64'));
}

// ---- derivations (spec §2.3, §2.4) ----
export const slot = (secret) => sha256(DOMAIN_SLOT, secret);
export const slotPda = (gate, slot_) => sha256(LEE_PDA_PREFIX, gate, slot_);
export const context = (kind, zone, gate, audience, payload) =>
  sha256(DOMAIN_CTX, Uint8Array.of(kind), zone, gate, audience, payload);
export const binding = (nft, collection, ctx) => sha256(DOMAIN_BIND, nft, collection, ctx);
export const payloadOnchain = (submissionId, actor, nonce) => sha256(submissionId, actor, u64le(nonce));
export const payloadOffchain = (purpose, nonce) => sha256(purpose, nonce);
export const audienceOffchain = (callerIdentity) => sha256(callerIdentity);
export const contextOnchain = (zone, gate, consumer, submissionId, actor, nonce) =>
  context(KIND_ONCHAIN, zone, gate, consumer, payloadOnchain(submissionId, actor, nonce));
export const contextOffchain = (zone, gate, callerIdentity, purpose, nonce) =>
  context(KIND_OFFCHAIN, zone, gate, audienceOffchain(callerIdentity), payloadOffchain(purpose, nonce));

/** The wallet rule: not_before = floor(now, QUANT_MS). */
export const notBeforeFor = (nowMs) => BigInt(nowMs) - (BigInt(nowMs) % QUANT_MS);
/** The wallet rule: the smallest TTL class covering requestedMs, or null. */
export function ttlClassFor(requestedMs) {
  const r = BigInt(requestedMs);
  const i = TTL_MS.findIndex((t) => t >= r);
  return i < 0 ? null : i;
}

// ---- state layout (spec §2.2) ----
/** Strict decode of a version-1 Fact as the gate writes it; null otherwise. */
export function decodeFact(b) {
  if (b.length !== FACT_LEN || b[0] !== 1 || b[1] !== 1) return null;
  const notBefore = readU64le(b, 34);
  const expiresAt = readU64le(b, 42);
  if (notBefore % QUANT_MS !== 0n || expiresAt < notBefore || !TTL_MS.includes(expiresAt - notBefore)) return null;
  return { binding: b.slice(2, 34), notBefore, expiresAt };
}
export function encodeFact({ binding: b, notBefore, expiresAt }) {
  return concat(Uint8Array.of(1, 1), b, u64le(notBefore), u64le(expiresAt));
}

function eq32(a, b) {
  if (a.length !== 32 || b.length !== 32) return false;
  let d = 0;
  for (let i = 0; i < 32; i++) d |= a[i] ^ b[i];
  return d === 0;
}

/**
 * The one check. `state`: the slot's gate actor state as read; `expected`: the binding the
 * verifier computed itself; `nowMs`: the chain time, or null (on-chain form, no clock).
 * Order: empty → NO_FACT; not a v1 fact → MALFORMED; other binding → BINDING_MISMATCH;
 * before not_before → NOT_YET_VALID; at or past expires_at → EXPIRED; else OK.
 */
export function check(state, expected, nowMs = null) {
  if (state.length === 0) return { name: 'NO_FACT', code: 1, fact: null };
  const fact = decodeFact(state);
  if (!fact) return { name: 'MALFORMED', code: 2, fact: null };
  if (!eq32(fact.binding, expected)) return { name: 'BINDING_MISMATCH', code: 3, fact };
  if (nowMs !== null && nowMs !== undefined) {
    const now = BigInt(nowMs);
    if (now < fact.notBefore) return { name: 'NOT_YET_VALID', code: 4, fact };
    if (now >= fact.expiresAt) return { name: 'EXPIRED', code: 5, fact };
  }
  return { name: 'OK', code: 0, fact };
}
