// SPDX-License-Identifier: MIT OR Apache-2.0
// Members' door, Basecamp skeleton: gate_core's check and derivations in JavaScript the QML
// engine can run (no BigInt, no TextEncoder, no crypto: bytes are plain arrays of numbers,
// as the Names Wallet's Testimonial app does). Times are Numbers (exact below 2^53 ms).
// Pinned to crates/gate_core by vectors/gate-vectors.json (miniapp/test/logic.test.mjs).

export var QUANT_MS = 600000
export var TTL_MS = [3600000, 21600000, 86400000]
export var CODES = ["OK", "NO_FACT", "MALFORMED", "BINDING_MISMATCH", "NOT_YET_VALID", "EXPIRED",
    "NOT_AUTHORIZED", "RPC_UNAVAILABLE", "NOT_A_MEMBER"]

export function utf8(text) {
    var out = []
    for (var i = 0; i < text.length; i++) {
        var c = text.charCodeAt(i)
        if (c >= 0xd800 && c <= 0xdbff && i + 1 < text.length) {
            var d = text.charCodeAt(i + 1)
            if (d >= 0xdc00 && d <= 0xdfff) { c = 0x10000 + ((c - 0xd800) << 10) + (d - 0xdc00); i++ } else { c = 0xfffd }
        } else if (c >= 0xd800 && c <= 0xdfff) { c = 0xfffd }
        if (c < 0x80) out.push(c)
        else if (c < 0x800) out.push(0xc0 | (c >> 6), 0x80 | (c & 63))
        else if (c < 0x10000) out.push(0xe0 | (c >> 12), 0x80 | ((c >> 6) & 63), 0x80 | (c & 63))
        else out.push(0xf0 | (c >> 18), 0x80 | ((c >> 12) & 63), 0x80 | ((c >> 6) & 63), 0x80 | (c & 63))
    }
    return out
}

export function hex(b) { var s = ""; for (var i = 0; i < b.length; i++) s += (b[i] < 16 ? "0" : "") + b[i].toString(16); return s }
export function unhex(s) { var out = []; for (var i = 0; i + 1 < s.length; i += 2) out.push(parseInt(s.substr(i, 2), 16)); return out }

var K = [0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2]

/** SHA-256 of the concatenation of `parts` (arrays of byte numbers). */
export function sha256(parts) {
    var m = []
    for (var p = 0; p < parts.length; p++) for (var q = 0; q < parts[p].length; q++) m.push(parts[p][q])
    var bits = m.length * 8
    m.push(0x80)
    while (m.length % 64 !== 56) m.push(0)
    var hi = Math.floor(bits / 4294967296), lo = bits >>> 0
    m.push((hi >>> 24) & 255, (hi >>> 16) & 255, (hi >>> 8) & 255, hi & 255, (lo >>> 24) & 255, (lo >>> 16) & 255, (lo >>> 8) & 255, lo & 255)
    var h = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19]
    var w = new Array(64)
    function rotr(x, n) { return (x >>> n) | (x << (32 - n)) }
    for (var off = 0; off < m.length; off += 64) {
        for (var i = 0; i < 16; i++) w[i] = ((m[off + 4 * i] << 24) | (m[off + 4 * i + 1] << 16) | (m[off + 4 * i + 2] << 8) | m[off + 4 * i + 3]) >>> 0
        for (i = 16; i < 64; i++) {
            var s0 = rotr(w[i - 15], 7) ^ rotr(w[i - 15], 18) ^ (w[i - 15] >>> 3)
            var s1 = rotr(w[i - 2], 17) ^ rotr(w[i - 2], 19) ^ (w[i - 2] >>> 10)
            w[i] = (w[i - 16] + s0 + w[i - 7] + s1) >>> 0
        }
        var a = h[0], b = h[1], c = h[2], d = h[3], e = h[4], f = h[5], g = h[6], hh = h[7]
        for (i = 0; i < 64; i++) {
            var t1 = (hh + (rotr(e, 6) ^ rotr(e, 11) ^ rotr(e, 25)) + ((e & f) ^ (~e & g)) + K[i] + w[i]) >>> 0
            var t2 = ((rotr(a, 2) ^ rotr(a, 13) ^ rotr(a, 22)) + ((a & b) ^ (a & c) ^ (b & c))) >>> 0
            hh = g; g = f; f = e; e = (d + t1) >>> 0; d = c; c = b; b = a; a = (t1 + t2) >>> 0
        }
        h[0] = (h[0] + a) >>> 0; h[1] = (h[1] + b) >>> 0; h[2] = (h[2] + c) >>> 0; h[3] = (h[3] + d) >>> 0
        h[4] = (h[4] + e) >>> 0; h[5] = (h[5] + f) >>> 0; h[6] = (h[6] + g) >>> 0; h[7] = (h[7] + hh) >>> 0
    }
    var out = []
    for (i = 0; i < 8; i++) out.push((h[i] >>> 24) & 255, (h[i] >>> 16) & 255, (h[i] >>> 8) & 255, h[i] & 255)
    return out
}

var B58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
export function b58encode(bytes) {
    var digits = []
    for (var i = 0; i < bytes.length; i++) {
        var carry = bytes[i]
        for (var j = 0; j < digits.length; j++) { carry += digits[j] * 256; digits[j] = carry % 58; carry = Math.floor(carry / 58) }
        while (carry > 0) { digits.push(carry % 58); carry = Math.floor(carry / 58) }
    }
    var s = ""
    for (i = 0; i < bytes.length && bytes[i] === 0; i++) s += "1"
    for (j = digits.length - 1; j >= 0; j--) s += B58[digits[j]]
    return s
}
export function b58decode32(s) {
    var out = []
    for (var i = 0; i < 32; i++) out.push(0)
    for (i = 0; i < s.length; i++) {
        var carry = B58.indexOf(s[i])
        if (carry < 0) throw new Error("bad base58")
        for (var j = 31; j >= 0; j--) { carry += out[j] * 58; out[j] = carry & 255; carry = Math.floor(carry / 256) }
        if (carry !== 0) throw new Error("longer than 32 bytes")
    }
    return out
}

function u64le(v) { var out = []; for (var i = 0; i < 8; i++) { out.push(v % 256); v = Math.floor(v / 256) } return out }
function readU64(b, at) { var v = 0; for (var i = 7; i >= 0; i--) v = v * 256 + b[at + i]; return v }

var DOMAIN_SLOT = utf8("lp0001-gate/v1/slot"), DOMAIN_CTX = utf8("lp0001-gate/v1/ctx"), DOMAIN_BIND = utf8("lp0001-gate/v1/bind")
var PDA_PREFIX = utf8("/LEE/v0.2/AccountId/PDA/").concat([0, 0, 0, 0, 0, 0, 0, 0])

export function slot(secret) { return sha256([DOMAIN_SLOT, secret]) }
export function slotPda(gate, slot_) { return sha256([PDA_PREFIX, gate, slot_]) }
export function context(kind, zone, gate, audience, payload) { return sha256([DOMAIN_CTX, [kind], zone, gate, audience, payload]) }
export function binding(nft, collection, ctx) { return sha256([DOMAIN_BIND, nft, collection, ctx]) }
export function contextOffchain(zone, gate, callerIdentity, purpose, nonce) {
    return context(2, zone, gate, sha256([callerIdentity]), sha256([purpose, nonce]))
}
export function contextOnchain(zone, gate, consumer, submissionId, actor, nonce) {
    return context(1, zone, gate, consumer, sha256([submissionId, actor, u64le(nonce)]))
}

/** check(state, expected, now|null) -> {name, code, fact}; the same order as gate_core. */
export function check(state, expected, now) {
    if (state.length === 0) return { name: "NO_FACT", code: 1, fact: null }
    if (state.length !== 50 || state[0] !== 1 || state[1] !== 1) return { name: "MALFORMED", code: 2, fact: null }
    var nb = readU64(state, 34), exp = readU64(state, 42)
    if (nb % QUANT_MS !== 0 || exp < nb || TTL_MS.indexOf(exp - nb) < 0) return { name: "MALFORMED", code: 2, fact: null }
    var fact = { binding: state.slice(2, 34), notBefore: nb, expiresAt: exp }
    for (var i = 0; i < 32; i++) if (state[2 + i] !== expected[i]) return { name: "BINDING_MISMATCH", code: 3, fact: fact }
    if (now !== null && now !== undefined) {
        if (now < nb) return { name: "NOT_YET_VALID", code: 4, fact: fact }
        if (now >= exp) return { name: "EXPIRED", code: 5, fact: fact }
    }
    return { name: "OK", code: 0, fact: fact }
}

/** Plain-language progress line for the QML view (no raw state codes). */
export function progressLine(state, phase, elapsedSeconds) {
  var t = Math.floor((elapsedSeconds || 0) / 60) + ':' + String((elapsedSeconds || 0) % 60).padStart(2, '0')
  if (state === 'pending') return 'Waiting for you to approve in the wallet…'
  if (state === 'running' && phase === 'prove') return 'Proving (about 10 minutes)… ' + t
  if (state === 'running') return 'Checking your NFT… ' + t
  if (state === 'done') return 'Ready.'
  if (state === 'failed') return 'Not done: the proof could not be completed.'
  if (state === 'rejected') return 'You declined the request. Nothing was sent.'
  if (state === 'expired') return 'The request expired. Try again.'
  if (state === 'cancelled') return 'You cancelled the request. Nothing was sent.'
  return 'Preparing…'
}

/** What a finished proof tells the user, without revealing the NFT or account. */
export function proofLine(proof) {
  if (!proof || typeof proof.slot !== 'string' || proof.slot.length !== 64) return 'Proof ready.'
  return 'Proof ready. Pass id ' + proof.slot.slice(0, 8) + '…; no NFT or account is revealed.'
}

/**
 * Which numbered step of the door guide is current: 1 ask, 2 approve, 3 read the pass.
 * A busy call or a waiting request holds the person on step 2; a finished proof moves to 3.
 * A refused, declined or withdrawn request stops the poll, so they are back at step 1.
 */
export function doorStep(v) {
  if (v.proofReady) return 3
  if (v.waiting || v.busy) return 2
  return 1
}

/** The "How it works" steps: verb + object labels, plain help, one marked as current. */
export function doorSteps(step) {
  var rows = [
    { n: 1, text: "Ask for a proof",
      detail: "Names Wallet asks you to prove you hold an NFT of the collection. This app never sees your account or which NFT you use." },
    { n: 2, text: "Approve in the wallet",
      detail: "The wallet shows what becomes public: a pass id and when it starts and ends. Proving takes about 10 minutes; you come back here after." },
    { n: 3, text: "Read your pass",
      detail: "Your pass id is below. Show it to an app that asks for it; it says nothing about which NFT or account you used." }
  ]
  return rows.map(function(row) {
    return { n: row.n, text: row.text, detail: row.detail,
      state: row.n < step ? "done" : row.n === step ? "now" : "later" }
  })
}

/** A plain stage or a ready pass stays in the page's ink; only a real problem is red. */
export function isProblem(line) {
  return /^(Not done:|Could not |You declined|The request expired|You cancelled|The wallet is busy\.)/.test(String(line || ""))
}

/** A short id for a heading; the full value stays in the technical details. */
export function short(id) {
  var s = String(id || "")
  return s.length <= 12 ? s : s.slice(0, 8) + "…"
}
