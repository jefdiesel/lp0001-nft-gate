// SPDX-License-Identifier: MIT OR Apache-2.0
// The `membership-proof` capability, dApp side (spec §4.1), over the wallet's provider module.
//
// The wallet (Names Wallet, `names` module) is to implement the module methods named in
// docs/WALLET-INTEGRATION.md; this shim gives a dApp the spec's API on top of them:
//
//   const mp = createMembershipClient(transport)
//   const { requestId } = await mp.requestMembershipProof({ collection, purpose, nonce, ttl, onchain })
//   const stop = mp.onProofStatus(requestId, (status) => render(status))
//
// `transport.call(method, ...args)` calls the wallet's module and resolves to its JSON text
// reply: {"success": bool, "value": …, "error": …, "busy"?: true} (the Names Wallet's
// envelope). In Basecamp QML that is `logos.callModuleAsync("names", method, args, cb)`.
//
// Privacy rule enforced here as well as in the wallet: a result carries only the proof object
// {gate, slot, not_before, expires_at, tx_hash} (and act_tx_hash for on-chain use). Any other
// field (an account, a balance, a holding, an alias, a commitment, a nullifier) is dropped and
// reported, never handed to the app.

export const METHODS = Object.freeze({
  request: 'request_membership_proof',
  status: 'membership_proof_status',
  cancel: 'cancel_membership_proof',
});

/** The statuses the dApp sees (spec §4.1). */
export const STATUS = Object.freeze([
  'awaiting-approval', 'checking', 'proving', 'sending', 'waiting-for-block', 'ready', 'failed',
]);

/** The wallet's error codes (spec §4.5) and their plain messages. */
export const ERRORS = Object.freeze({
  NOT_A_MEMBER: "This wallet holds no NFT of that collection, so there is nothing to prove.",
  FEE_RESERVE: 'The account paying the fee does not hold the fee reserve.',
  ACCOUNT_BUSY: 'Your account changed while proving; try again.',
  OUT_OF_WINDOW: 'The proof landed too late for its validity window; prove again.',
  PROVER_OUT_OF_MEMORY: 'This computer does not have enough memory to make the proof.',
  RPC_UNREACHABLE: "Could not reach the chain's RPC.",
  GATE_NOT_PINNED: 'The gate or NFT program on chain is not the pinned, immutable one.',
  UNEXPECTED_OUTCOME: 'The proof would have done something other than write one pass; it was stopped.',
  REJECTED: 'You declined the request.',
  EXPIRED: 'Nobody answered the request in time.',
  CANCELLED: 'The request was withdrawn.',
});

const PROOF_FIELDS = ['gate', 'slot', 'not_before', 'expires_at', 'tx_hash', 'act_tx_hash'];
const B58 = /^[1-9A-HJ-NP-Za-km-z]{32,44}$/;
const HEX32 = /^[0-9a-f]{64}$/;
const MAX_TTL_MS = 86_400_000;

/** Validates a request before it reaches the wallet; throws with a plain message. */
export function validateRequest({ collection, purpose, nonce, ttl, onchain }) {
  if (typeof collection !== 'string' || !B58.test(collection)) throw new Error('collection: an account id in base58');
  if (typeof purpose !== 'string' || purpose.length === 0 || purpose.length > 280) throw new Error('purpose: 1 to 280 characters');
  if (typeof nonce !== 'string' || !HEX32.test(nonce)) throw new Error('nonce: 32 random bytes as 64 lower-case hex');
  if (!Number.isInteger(ttl) || ttl <= 0 || ttl > MAX_TTL_MS) throw new Error('ttl: milliseconds, at most 24 hours');
  if (onchain !== undefined) {
    if (!onchain || typeof onchain.consumer !== 'string' || !B58.test(onchain.consumer)) throw new Error('onchain.consumer: a program id in base58');
    const extra = Object.keys(onchain).filter((k) => k !== 'consumer');
    if (extra.length) throw new Error(`onchain: unknown field ${extra[0]} (the acting account is chosen in the wallet)`);
  }
  // There is deliberately no field for an account, an audience or a context: the wallet picks
  // the holding, fills the audience from who is calling, and builds the context itself.
  return { collection, purpose, nonce, ttl, ...(onchain ? { onchain: { consumer: onchain.consumer } } : {}) };
}

/** Keeps only the proof object's fields; reports anything else. */
export function sanitizeProof(value) {
  const proof = {};
  const dropped = [];
  for (const [k, v] of Object.entries(value ?? {})) {
    if (PROOF_FIELDS.includes(k)) proof[k] = v;
    else dropped.push(k);
  }
  return { proof, dropped };
}

/** Maps a wallet request record onto the dApp status (spec §4.1). */
export function toStatus(record) {
  const { state, phase, elapsed_seconds: elapsed, proof, error_code: code } = record;
  switch (state) {
    case 'pending': return { status: 'awaiting-approval' };
    case 'running':
      if (phase === 'check' || phase === 'simulate') return { status: 'checking' };
      if (phase === 'prove') return { status: 'proving', elapsedSeconds: elapsed ?? 0 };
      if (phase === 'submit' || phase === 'act') return { status: 'sending' };
      if (phase === 'confirm') return { status: 'waiting-for-block' };
      return { status: 'checking' };
    case 'done': {
      const { proof: p, dropped } = sanitizeProof(proof);
      return { status: 'ready', proof: p, ...(dropped.length ? { dropped } : {}) };
    }
    case 'failed': {
      const c = ERRORS[code] ? code : 'UNEXPECTED_OUTCOME';
      return { status: 'failed', code: c, plainMessage: ERRORS[c] };
    }
    case 'rejected': return { status: 'failed', code: 'REJECTED', plainMessage: ERRORS.REJECTED };
    case 'expired': return { status: 'failed', code: 'EXPIRED', plainMessage: ERRORS.EXPIRED };
    case 'cancelled': return { status: 'failed', code: 'CANCELLED', plainMessage: ERRORS.CANCELLED };
    default: return { status: 'failed', code: 'UNEXPECTED_OUTCOME', plainMessage: `unknown state ${state}` };
  }
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/**
 * A client over a transport. Options: `retries` on busy (default 6), `backoffMs` (500, doubling),
 * `intervalMs` between status reads (5000), `isVisible()` (status is read only while the app is
 * open and in front; default: always).
 */
export function createMembershipClient(transport, opts = {}) {
  const retries = opts.retries ?? 6;
  const backoffMs = opts.backoffMs ?? 500;
  const intervalMs = opts.intervalMs ?? 5000;
  const isVisible = opts.isVisible ?? (() => true);

  async function call(method, ...args) {
    for (let attempt = 0; ; attempt++) {
      const reply = JSON.parse(await transport.call(method, ...args));
      // busy: nothing happened, retry (the Names Wallet's rule: every app must retry on busy)
      if (reply.busy && attempt < retries) { await sleep(backoffMs * 2 ** attempt); continue; }
      if (!reply.success) throw new Error(reply.error ?? `${method} failed`);
      return reply.value;
    }
  }

  return {
    async requestMembershipProof(request) {
      const body = validateRequest(request);
      const value = await call(METHODS.request, JSON.stringify(body));
      if (!value || typeof value.request !== 'string') throw new Error('the wallet returned no request id');
      return { requestId: value.request };
    },
    async status(requestId) {
      return toStatus(await call(METHODS.status, requestId));
    },
    async cancel(requestId) {
      return toStatus(await call(METHODS.cancel, requestId));
    },
    /** Calls `cb(status)` on every change until ready or failed; returns stop(). */
    onProofStatus(requestId, cb) {
      let stopped = false;
      let last = '';
      (async () => {
        while (!stopped) {
          if (isVisible()) {
            let s;
            try { s = await this.status(requestId); } catch (e) { s = { status: 'failed', code: 'RPC_UNREACHABLE', plainMessage: String(e.message ?? e) }; }
            const key = JSON.stringify(s);
            if (key !== last) { last = key; cb(s); }
            if (s.status === 'ready' || s.status === 'failed') return;
          }
          await sleep(intervalMs);
        }
      })();
      return () => { stopped = true; };
    },
  };
}
