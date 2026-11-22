// SPDX-License-Identifier: MIT OR Apache-2.0
// Members' door, Basecamp app logic: gate_core's check and derivations in JavaScript the QML
// engine can run (no BigInt, no TextEncoder, no crypto, no Java reserved words: bytes are plain
// arrays of numbers). Times are Numbers (exact below 2^53 ms).
// Pinned to crates/gate_core by vectors/gate-vectors.json (miniapp/test/logic.test.mjs).
// Wallet contract matches Names 0.18.3 (`provider_proof.rs` app_view / Status::view / WalletError).
// Wallet plumbing (find/open Names, busy retry) is not here: Main.qml uses the copied
// Names Wallet SDK (./NamesWallet.qml and ./NamesWalletSDK.mjs) for that.

export var QUANT_MS = 600000
export var TTL_MS = [3600000, 21600000, 86400000]
export var MAX_PURPOSE = 120
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

// ── Plain-language wallet errors, states and privacy helpers (Names 0.18.3 contract) ──

/** Exact plain messages for unknown errors, failures after proving began, and cancels after proving. */
export var UNKNOWN_FAILURE_LINE = "Not finished. It may have been sent: check in Names before trying again."
export var CANCELLED_AFTER_PROVING_LINE = "Cancelled in Names. If proving had started it may still be sent: check in Names."
export var PROOF_UNCHECKED_LINE = "Proof sent. Not checked on chain by this app yet."
export var REQUEST_REFUSED_BEFORE_ID_LINE = "The wallet refused the request before it started. Nothing was sent."

var REAL_ONCHAIN_REFUSAL = "on-chain use isn't built in this wallet yet"
var REAL_NFT_UNLISTED_REFUSAL = "that nft program isn't in this network's list"

/**
 * Every WalletError code from Names 0.18.3 (`excerpts.rs`) plus request-level terminal codes.
 * Only codes where the wallet is known not to have sent anything say "Nothing was sent."
 * (and only when proving never began).
 */
export var WALLET_ERRORS = {
    NOT_A_MEMBER: "This account holds no NFT of that collection, so there is nothing to prove. Nothing was sent.",
    FEE_RESERVE: "The account paying the fee does not hold the fee reserve (about 5 million). Fund it first; that is one more private transaction.",
    ACCOUNT_BUSY: "Your account changed while proving, or is busy with another transaction. Try again.",
    OUT_OF_WINDOW: "The proof landed too late for its validity window. Prove again.",
    PROVER_OUT_OF_MEMORY: "This computer does not have enough memory to make the proof (a 16 GB machine is needed).",
    RPC_UNREACHABLE: "Could not reach the chain. Nothing was sent.",
    GATE_NOT_PINNED: "The membership gate on this network is not the pinned, unchangeable one (or none is set up yet). Nothing was sent.",
    UNEXPECTED_OUTCOME: "The proven transaction would do something other than write one pass and pay the fee. It was stopped and nothing was sent.",
    REJECTED: "You declined the request. Nothing was sent.",
    EXPIRED: "The request expired before it was approved. Nothing was sent.",
    CANCELLED: "Cancelled in Names.",
    ONCHAIN_UNSUPPORTED: "Names Wallet does not support on-chain membership proofs in this build yet. Use Check in this app instead."
}

/** Every gate_core verifier outcome code in plain words. */
export var VERDICT_WORDS = {
    OK: "Checked on chain: a current membership proof for this collection and this use.",
    NO_FACT: "No proof: this pass id has no proof written to it.",
    MALFORMED: "Not a proof: the proof data has an invalid shape.",
    BINDING_MISMATCH: "Not for this use: the proof is for another collection or use, or a newer proof replaced it.",
    NOT_YET_VALID: "Not valid yet: the proof's start time is still ahead.",
    EXPIRED: "Expired: the proof's validity window has ended.",
    NOT_AUTHORIZED: "Not authorised: the acting account did not sign.",
    RPC_UNAVAILABLE: "Could not read the chain: no answer from the RPC.",
    NOT_A_MEMBER: "Not a member: the wallet found no NFT of this collection to prove with."
}

var PROOF_FIELDS = ["gate", "slot", "not_before", "expires_at", "tx_hash", "act_tx_hash"]
var B58_ID_RE = /^[1-9A-HJ-NP-Za-km-z]{32,44}$/
var HEX32_RE = /^[0-9a-f]{64}$/
var TX_HASH_RE = /^[0-9a-fA-F]{64}$/
var CONTROL_RE = /[\u0000-\u001f\u007f-\u009f]/
var MAX_TTL_MS = 86400000

function isValidBase58Account(text) {
    if (typeof text !== "string") return false
    var s = text.trim()
    if (!B58_ID_RE.test(s)) return false
    try { b58decode32(s); return true } catch (e) { return false }
}

function charCount(text) {
    var s = String(text || "")
    var count = 0
    for (var i = 0; i < s.length; i++) {
        var c = s.charCodeAt(i)
        if (c >= 0xd800 && c <= 0xdbff && i + 1 < s.length) {
            var d = s.charCodeAt(i + 1)
            if (d >= 0xdc00 && d <= 0xdfff) i++
        }
        count++
    }
    return count
}

/** Format elapsed seconds as m:ss. */
export function formatElapsed(seconds) {
    var secs = Math.floor(Number(seconds) || 0)
    if (secs < 0) secs = 0
    var mins = Math.floor(secs / 60)
    var rem = secs % 60
    return mins + ":" + (rem < 10 ? "0" + rem : String(rem))
}

/** Format a Unix timestamp in milliseconds as YYYY-MM-DD HH:MM UTC (no BigInt). */
export function formatUtcTime(ms) {
    var n = Number(ms)
    if (!n || isNaN(n) || n <= 0) return ""
    var dt = new Date(n)
    if (isNaN(dt.getTime())) return ""
    function pad2(v) { return v < 10 ? "0" + v : String(v) }
    return dt.getUTCFullYear() + "-" + pad2(dt.getUTCMonth() + 1) + "-" + pad2(dt.getUTCDate())
        + " " + pad2(dt.getUTCHours()) + ":" + pad2(dt.getUTCMinutes()) + " UTC"
}

/**
 * Map an exact wallet/verifier code (or the wallet's exact on-chain refusal) to plain words.
 * Never guesses by substrings. Any failure after proving began, or any unknown error, returns
 * UNKNOWN_FAILURE_LINE (never claiming "Nothing was sent").
 */
export function plainError(code, rawMessage, opts) {
    var o = opts || {}
    if (o.afterProving) return UNKNOWN_FAILURE_LINE

    var key = String(code || "").trim().toUpperCase()
    if (key && WALLET_ERRORS[key]) return WALLET_ERRORS[key]
    if (key && key !== "OK" && VERDICT_WORDS[key]) return VERDICT_WORDS[key]

    var lower = String(rawMessage || "").toLowerCase()
    if (lower.indexOf(REAL_ONCHAIN_REFUSAL) >= 0) return WALLET_ERRORS.ONCHAIN_UNSUPPORTED
    if (o.beforeId && lower.indexOf(REAL_NFT_UNLISTED_REFUSAL) >= 0) {
        return "That NFT program is not in Names Wallet's list for this network: add it in Names under Collectibles, Programs. Nothing was sent."
    }
    if (o.beforeId) return REQUEST_REFUSED_BEFORE_ID_LINE
    return UNKNOWN_FAILURE_LINE
}

/**
 * Map a refusal on an on-chain proof request.
 * Only the wallet's real refusal ("on-chain use isn't built in this wallet yet") maps to
 * ONCHAIN_UNSUPPORTED; an unknown error stays unknown.
 */
export function onchainRefusalMessage(code, rawMessage, opts) {
    return plainError(code, rawMessage, opts)
}

/** Keep only LP-0001 proof fields; drop anything else (account, balance, holding, commitment, nullifier). */
export function sanitizeProof(raw) {
    var out = {}
    var dropped = []
    if (raw && typeof raw === "object") {
        var keys = Object.keys(raw)
        for (var i = 0; i < keys.length; i++) {
            var k = keys[i]
            if (PROOF_FIELDS.indexOf(k) >= 0) out[k] = raw[k]
            else dropped.push(k)
        }
    }
    return { proof: out, dropped: dropped }
}

/**
 * Validate the shape of a wallet-returned proof (`gate`, `slot`, `tx_hash`, `not_before`, `expires_at`,
 * and optional `act_tx_hash`). Shape validation is NOT chain verification: it only rejects malformed data.
 */
export function validateProofShape(raw) {
    var clean = sanitizeProof(raw)
    var p = clean.proof
    if (!isValidBase58Account(p.gate)) return { ok: false, reason: "gate", proof: null, dropped: clean.dropped }
    if (typeof p.slot !== "string" || !HEX32_RE.test(p.slot)) return { ok: false, reason: "slot", proof: null, dropped: clean.dropped }
    if (typeof p.tx_hash !== "string" || !TX_HASH_RE.test(p.tx_hash)) return { ok: false, reason: "tx_hash", proof: null, dropped: clean.dropped }
    if (p.act_tx_hash !== undefined && p.act_tx_hash !== null) {
        if (typeof p.act_tx_hash !== "string" || !TX_HASH_RE.test(p.act_tx_hash)) {
            return { ok: false, reason: "act_tx_hash", proof: null, dropped: clean.dropped }
        }
    }
    var nb = p.not_before
    var exp = p.expires_at
    if (typeof nb !== "number" || !isFinite(nb) || Math.floor(nb) !== nb || nb <= 0 || nb % QUANT_MS !== 0) {
        return { ok: false, reason: "window", proof: null, dropped: clean.dropped }
    }
    if (typeof exp !== "number" || !isFinite(exp) || Math.floor(exp) !== exp || exp <= nb || TTL_MS.indexOf(exp - nb) < 0) {
        return { ok: false, reason: "window", proof: null, dropped: clean.dropped }
    }
    return { ok: true, reason: "", proof: p, dropped: clean.dropped }
}

/**
 * Build a Names 0.18.3 membership-proof request body (`program`, `collection`, `purpose`, `nonce`,
 * `ttl`, optional `onchain`). Never includes account, audience or context fields.
 * Returns { ok: true, body } or { ok: false, error }.
 */
export function buildProofRequest(opts) {
    var o = opts || {}
    var program = String(o.program || "").trim()
    var collection = String(o.collection || "").trim()
    var purpose = String(o.purpose || "").trim()
    var nonce = String(o.nonce || "").trim()
    var ttl = Number(o.ttl)
    if (!isValidBase58Account(program)) return { ok: false, error: "No NFT program is set up for this network yet. Nothing was sent." }
    if (!isValidBase58Account(collection)) return { ok: false, error: "Enter a valid collection id first. Nothing was sent." }
    if (!purpose || charCount(purpose) > MAX_PURPOSE || CONTROL_RE.test(purpose)) {
        return { ok: false, error: "Purpose must be plain text of 1 to 120 characters. Nothing was sent." }
    }
    if (!HEX32_RE.test(nonce)) return { ok: false, error: "Could not prepare a fresh request nonce. Nothing was sent." }
    if (!ttl || isNaN(ttl) || ttl <= 0 || ttl > MAX_TTL_MS || Math.floor(ttl) !== ttl) {
        return { ok: false, error: "Validity time must be a whole number of milliseconds, up to 24 hours. Nothing was sent." }
    }
    var body = { program: program, collection: collection, purpose: purpose, nonce: nonce, ttl: ttl }
    if (o.onchain) {
        var consumer = String(o.consumer || "").trim()
        if (!isValidBase58Account(consumer)) return { ok: false, error: "Add a valid consumer program id before trying on-chain use. Nothing was sent." }
        body.onchain = { consumer: consumer }
    }
    return { ok: true, body: body }
}

/** Plain-language progress line for a real Names 0.18.3 `status.state` (or top-level terminal state). */
export function progressLine(stateName, elapsedSeconds) {
    var t = formatElapsed(elapsedSeconds)
    if (stateName === "awaiting-approval") return "Waiting for you to approve in Names Wallet…"
    if (stateName === "checking") return "Checking your NFT…"
    if (stateName === "proving") return "Proving (about 10 minutes)… " + t
    if (stateName === "sending") return "Sending…"
    if (stateName === "waiting-for-block") return "Sent; waiting for the block…"
    if (stateName === "ready") return PROOF_UNCHECKED_LINE
    if (stateName === "rejected") return WALLET_ERRORS.REJECTED
    if (stateName === "expired") return WALLET_ERRORS.EXPIRED
    if (stateName === "cancelled") return WALLET_ERRORS.CANCELLED
    return "Preparing…"
}

/** Plain message when reading `membership_proof_status` itself failed (the proof may still be running). */
export function statusReadErrorLine(err) {
    if (err && err.busy) return "Names Wallet is busy. Still checking the request status…"
    if (err && err.missing) return "Could not read the status because Names Wallet is not answering. Still checking; do not start another proof while this one may be running."
    return "Could not read the request status. Still checking; do not start another proof while this one may be running."
}

/**
 * Structured status view for a Names 0.18.3 `app_view()` record from `membership_proof_status`
 * or `cancel_membership_proof`.
 *
 * Top-level `state`: pending | approved | done | failed | cancelled | rejected | expired.
 * Nested `status.state`: awaiting-approval | checking | proving (elapsed_seconds) | sending |
 *   waiting-for-block | ready (proof) | failed (code, plain_message) | cancelled.
 *
 * Never returns "Valid": this app has no chain reader yet, so a well-shaped `ready` proof
 * becomes neutral `kind: "unverified"` ("Proof sent. Not checked on chain by this app yet.").
 */
export function describeStatus(record, fallbackElapsed, opts) {
    var r = record && typeof record === "object" ? record : null
    var o = (opts && typeof opts === "object") ? opts : { onchain: !!opts, provingBegan: false }
    var provingBegan = o.provingBegan === true

    if (!r) {
        return {
            kind: "status-error", badge: "Status unreadable", line: statusReadErrorLine(null),
            isProblem: true, active: true, canCancel: false, done: false,
            provingStarted: false, statusReadFailed: true, elapsedSeconds: 0, code: "", proof: null
        }
    }

    var topState = typeof r.state === "string" ? r.state : ""
    var nested = (r.status && typeof r.status === "object") ? r.status : null
    var nestedState = nested && typeof nested.state === "string" ? nested.state : ""

    // Top-level terminal outcomes that can appear even while nested status still says awaiting-approval.
    if (topState === "rejected") {
        return {
            kind: "refused", badge: "Refused", line: WALLET_ERRORS.REJECTED,
            isProblem: true, active: false, canCancel: false, done: true,
            provingStarted: false, statusReadFailed: false, elapsedSeconds: 0, code: "REJECTED", proof: null
        }
    }
    if (topState === "expired") {
        return {
            kind: "refused", badge: "Refused", line: WALLET_ERRORS.EXPIRED,
            isProblem: true, active: false, canCancel: false, done: true,
            provingStarted: false, statusReadFailed: false, elapsedSeconds: 0, code: "EXPIRED", proof: null
        }
    }
    if (topState === "cancelled" || nestedState === "cancelled") {
        return {
            kind: "refused", badge: "Cancelled",
            // A poll every few seconds can miss the start of proving, so always say it may still be sent.
            line: CANCELLED_AFTER_PROVING_LINE,
            isProblem: true, active: false, canCancel: false, done: true,
            provingStarted: false, statusReadFailed: false, elapsedSeconds: 0, code: "CANCELLED", proof: null
        }
    }

    if (nestedState === "awaiting-approval") {
        return {
            kind: "waiting", badge: "Waiting", line: progressLine("awaiting-approval", 0),
            isProblem: false, active: true, canCancel: true, done: false,
            provingStarted: false, statusReadFailed: false, elapsedSeconds: 0, code: "", proof: null
        }
    }
    if (nestedState === "checking") {
        return {
            kind: "checking", badge: "Checking", line: progressLine("checking", 0),
            isProblem: false, active: true, canCancel: true, done: false,
            provingStarted: false, statusReadFailed: false, elapsedSeconds: 0, code: "", proof: null
        }
    }
    if (nestedState === "proving") {
        var hasElapsed = nested.elapsed_seconds !== undefined && nested.elapsed_seconds !== null && !isNaN(Number(nested.elapsed_seconds))
        var secs = hasElapsed ? Math.max(0, Math.floor(Number(nested.elapsed_seconds))) : Math.max(0, Math.floor(Number(fallbackElapsed) || 0))
        return {
            kind: "proving", badge: "Proving", line: progressLine("proving", secs),
            isProblem: false, active: true, canCancel: true, done: false,
            provingStarted: true, statusReadFailed: false, elapsedSeconds: secs, code: "", proof: null
        }
    }
    if (nestedState === "sending") {
        return {
            kind: "sending", badge: "Sending", line: progressLine("sending", 0),
            isProblem: false, active: true, canCancel: false, done: false,
            provingStarted: true, statusReadFailed: false, elapsedSeconds: 0, code: "", proof: null
        }
    }
    if (nestedState === "waiting-for-block") {
        return {
            kind: "sent", badge: "Sent", line: progressLine("waiting-for-block", 0),
            isProblem: false, active: true, canCancel: false, done: false,
            provingStarted: true, statusReadFailed: false, elapsedSeconds: 0, code: "", proof: null
        }
    }
    if (nestedState === "ready") {
        var shape = validateProofShape(nested.proof)
        if (!shape.ok) {
            return {
                kind: "refused", badge: "Refused", line: "Not done: " + VERDICT_WORDS.MALFORMED,
                isProblem: true, active: false, canCancel: false, done: true,
                provingStarted: false, statusReadFailed: false, elapsedSeconds: 0, code: "MALFORMED", proof: null
            }
        }
        return {
            kind: "unverified", badge: "Unchecked", line: PROOF_UNCHECKED_LINE,
            isProblem: false, active: false, canCancel: false, done: true,
            provingStarted: false, statusReadFailed: false, elapsedSeconds: 0, code: "", proof: shape.proof
        }
    }
    if (nestedState === "failed" || topState === "failed") {
        var code = (nested && typeof nested.code === "string") ? nested.code : ""
        var rawMsg = (nested && typeof nested.plain_message === "string") ? nested.plain_message : ""
        var mapped = o.onchain
            ? onchainRefusalMessage(code, rawMsg, { afterProving: provingBegan })
            : plainError(code, rawMsg, { afterProving: provingBegan })
        var line = mapped === UNKNOWN_FAILURE_LINE ? mapped : ("Not done: " + mapped)
        return {
            kind: "refused", badge: "Refused", line: line,
            isProblem: true, active: false, canCancel: false, done: true,
            provingStarted: false, statusReadFailed: false, elapsedSeconds: 0, code: code, proof: null
        }
    }
    if (topState === "done") {
        return {
            kind: "refused", badge: "Refused", line: "Not done: " + VERDICT_WORDS.MALFORMED,
            isProblem: true, active: false, canCancel: false, done: true,
            provingStarted: false, statusReadFailed: false, elapsedSeconds: 0, code: "MALFORMED", proof: null
        }
    }

    // Unrecognized reply shape: treat as unreadable status, NOT as a failed proof.
    return {
        kind: "status-error", badge: "Status unreadable", line: statusReadErrorLine(null),
        isProblem: true, active: true, canCancel: false, done: false,
        provingStarted: false, statusReadFailed: true, elapsedSeconds: 0, code: "", proof: null
    }
}

/** Initial state for one proof-request session (used by Main.qml and unit tests). */
export function createRequestSession() {
    return {
        polling: false,
        provingBegan: false,
        statusKind: "",
        statusBadge: "",
        stage: "",
        canCancel: false,
        elapsedSeconds: 0,
        lastProof: null,
        statusReadWarning: "",
        actionsEnabled: true
    }
}

/** Apply a `membership_proof_status` or `cancel_membership_proof` answer to the current session. */
export function applyPollSuccess(session, record, fallbackElapsed, onchainMode) {
    var prev = session || createRequestSession()
    var view = describeStatus(record, fallbackElapsed, {
        provingBegan: prev.provingBegan === true,
        onchain: !!onchainMode
    })
    if (view.statusReadFailed) {
        return {
            polling: true,
            provingBegan: prev.provingBegan === true,
            statusKind: prev.statusKind || "waiting",
            statusBadge: prev.statusBadge || "Waiting",
            stage: prev.stage || progressLine("awaiting-approval", 0),
            canCancel: prev.canCancel === true,
            elapsedSeconds: prev.elapsedSeconds || 0,
            lastProof: prev.lastProof || null,
            statusReadWarning: view.line,
            actionsEnabled: false
        }
    }
    var began = prev.provingBegan === true || view.provingStarted === true
    var finished = view.done === true
    return {
        polling: !finished,
        provingBegan: began,
        statusKind: view.kind,
        statusBadge: view.badge,
        stage: view.line,
        canCancel: view.canCancel === true,
        elapsedSeconds: view.elapsedSeconds || 0,
        lastProof: view.kind === "unverified" ? view.proof : null,
        statusReadWarning: "",
        actionsEnabled: finished
    }
}

/**
 * Apply a failed `membership_proof_status` read: never fails the proof, keeps polling,
 * shows that the status could not be read, and never re-enables a second request.
 */
export function applyPollFailure(session, err) {
    var prev = session || createRequestSession()
    return {
        polling: true,
        provingBegan: prev.provingBegan === true,
        statusKind: prev.statusKind || "waiting",
        statusBadge: prev.statusBadge || "Waiting",
        stage: prev.stage || progressLine("awaiting-approval", 0),
        canCancel: prev.canCancel === true,
        elapsedSeconds: prev.elapsedSeconds || 0,
        lastProof: prev.lastProof || null,
        statusReadWarning: statusReadErrorLine(err),
        actionsEnabled: false
    }
}

/** What a returned proof tells the user, without revealing the NFT or account or claiming verification. */
export function proofLine(proof) {
    var shape = validateProofShape(proof)
    if (!shape.ok) return PROOF_UNCHECKED_LINE
    return PROOF_UNCHECKED_LINE + " Pass id " + shape.proof.slot.slice(0, 8) + "…; no NFT or account is revealed."
}

/**
 * What the returned pass card shows. Because this app has no chain reader yet, it NEVER reports
 * "Valid" or "Establishes: holder of": it shows the shape-checked pass details neutrally.
 */
export function describePass(proof, collectionLabel) {
    var shape = validateProofShape(proof)
    if (!shape.ok) {
        return {
            shapeValid: false,
            verified: false,
            headline: "Malformed pass data",
            statusLine: VERDICT_WORDS.MALFORMED,
            passIdShort: "",
            passIdFull: "",
            windowLine: "",
            reveals: "",
            doesNotReveal: "Which NFT you hold, which account holds it, your balances, or your other NFTs.",
            onchainLine: "",
            trustNote: "The wallet reply did not match the expected pass shape."
        }
    }
    var p = shape.proof
    var fromText = formatUtcTime(p.not_before)
    var untilText = formatUtcTime(p.expires_at)
    var hasAct = typeof p.act_tx_hash === "string" && p.act_tx_hash.length > 0
    return {
        shapeValid: true,
        verified: false,
        headline: "Proof sent (not checked on chain)",
        statusLine: PROOF_UNCHECKED_LINE,
        collectionLabel: String(collectionLabel || ""),
        passIdShort: shortId(p.slot),
        passIdFull: p.slot,
        windowLine: fromText && untilText ? (fromText + " to " + untilText) : "",
        reveals: hasAct
            ? "A pass id, when it starts and ends, and the public acting account you chose in Names."
            : "A pass id and when it starts and ends. Two apps shown this pass id can tell it is the same holder, not who you are.",
        doesNotReveal: "Which NFT you hold, which account holds it, your balances, or your other NFTs.",
        onchainLine: hasAct ? ("Act tx reported by Names (" + shortId(p.act_tx_hash) + "); not checked on chain by this app.") : "",
        trustNote: "This app has no chain reader yet, so it has not checked this pass on chain."
    }
}

/**
 * Which numbered step of the door guide is current: 1 ask, 2 approve, 3 read the returned pass.
 * A busy call or a waiting request holds the person on step 2; a returned pass moves to 3.
 * A refused, declined or cancelled request stops the poll, so they are back at step 1.
 */
export function doorStep(v) {
    if (v.proofReturned || v.proofReady) return 3
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
        { n: 3, text: "Read the returned pass",
          detail: "The pass id returned by Names appears below. This app has no chain reader yet, so it cannot verify the pass on chain itself." }
    ]
    return rows.map(function(row) {
        return { n: row.n, text: row.text, detail: row.detail,
            state: row.n < step ? "done" : row.n === step ? "now" : "later" }
    })
}

/** Immediate button label while a proof request is running. */
export function buttonBusyLabel(statusKind, elapsedSeconds) {
    if (statusKind === "proving") return "Proving… " + formatElapsed(elapsedSeconds)
    if (statusKind === "checking") return "Checking…"
    if (statusKind === "sending") return "Sending…"
    if (statusKind === "sent") return "Waiting for block…"
    if (statusKind === "waiting") return "Waiting in Names…"
    return "Preparing…"
}

/** A plain stage or an unchecked pass stays in the page's ink; only a real problem is red. */
export function isProblem(line) {
    return /^(Not done:|Not finished\.|Could not |You declined|The request expired|The request was refused|Cancelled in Names|The wallet |Names Wallet (is busy|does not support|didn't answer)|This account holds|The account paying|Your account changed|The proof landed|This computer does not|The membership gate|The proven transaction|That NFT program|No proof:|Not a proof:|Not for this use:|Not valid yet:|Expired:|Not authorised:|Not a member:)/.test(String(line || ""))
}

/** A short id for a heading; the full value stays in Technical details. */
export function shortId(id) {
    var s = String(id || "")
    return s.length <= 12 ? s : s.slice(0, 8) + "…"
}
