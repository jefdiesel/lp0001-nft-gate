// SPDX-License-Identifier: MIT OR Apache-2.0
// UX, wallet-contract, privacy, config and Basecamp packaging regression tests.
// Fixtures match Names 0.18.3's Rust views in .ref/wallet/provider_proof.rs and excerpts.rs
// (exact JSON with nested `status`). No Qt runner is needed.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

import {
  MAX_PURPOSE,
  UNKNOWN_FAILURE_LINE,
  CANCELLED_AFTER_PROVING_LINE,
  PROOF_UNCHECKED_LINE,
  REQUEST_REFUSED_BEFORE_ID_LINE,
  WALLET_ERRORS,
  VERDICT_WORDS,
  progressLine as qmlProgress,
  proofLine,
  doorStep,
  doorSteps,
  isProblem,
  shortId,
  formatElapsed,
  formatUtcTime,
  plainError,
  onchainRefusalMessage,
  sanitizeProof,
  validateProofShape,
  buildProofRequest,
  statusReadErrorLine,
  describeStatus,
  createRequestSession,
  applyPollSuccess,
  applyPollFailure,
  describePass,
  buttonBusyLabel,
} from '../basecamp/qml/logic.mjs';
import {
  DEFAULT_NETWORK,
  NETWORKS,
  isBase58Id,
  networkFor,
  describeUnknownZone,
  resolveConfig,
} from '../basecamp/qml/networks.mjs';
import { progressLine as webProgress, PendingGuard } from '../web/logic.mjs';

const VALID_ID_A = 'CjrUpegdA1RjqDN43T8P19kAkwnD1quAkNkER8k3dw9G';
const VALID_ID_B = 'ErycMYqXUUtducaYmpHZjRgdbLXchrMiV4NZRnmRDNmu';
const VALID_ID_C = 'HA47ERcmfSbDChGcYfBcC4SKaPQ8pJJnv4wxCLu1ejL5';
const VALID_HEX32 = 'ab'.repeat(32);
const VALID_TX_HASH = 'cd'.repeat(32);
const VALID_ACT_TX_HASH = 'ef'.repeat(32);
const VALID_NOT_BEFORE = 1763500200000; // multiple of 600_000 ms
const VALID_EXPIRES_AT = 1763503800000; // + 3_600_000 ms (1 hour)

/** Exact shape of `provider_proof::app_view(request)` from Names 0.18.3. */
function appView(topState, nestedStatus, extra) {
  return Object.assign({
    request: 'req-01',
    kind: 'membership-proof',
    state: topState,
    collection: VALID_ID_C,
    status: nestedStatus,
    created: 1763500200000,
    expires: 1763500800000,
  }, extra || {});
}

function validProof(extra) {
  return Object.assign({
    gate: VALID_ID_B,
    slot: VALID_HEX32,
    not_before: VALID_NOT_BEFORE,
    expires_at: VALID_EXPIRES_AT,
    tx_hash: VALID_TX_HASH,
  }, extra || {});
}

test('Names 0.18.3 app_view(): every nested status.state and top-level state is handled accurately', () => {
  const waiting = describeStatus(appView('pending', { state: 'awaiting-approval' }), 0, {});
  assert.equal(waiting.kind, 'waiting');
  assert.equal(waiting.badge, 'Waiting');
  assert.equal(waiting.canCancel, true);
  assert.equal(waiting.done, false);
  assert.equal(waiting.active, true);
  assert.equal(waiting.provingStarted, false);

  const checking = describeStatus(appView('approved', { state: 'checking' }), 0, {});
  assert.equal(checking.kind, 'checking');
  assert.equal(checking.badge, 'Checking');
  assert.equal(checking.canCancel, true);
  assert.equal(checking.done, false);
  assert.equal(checking.provingStarted, false);

  const proving = describeStatus(appView('approved', { state: 'proving', elapsed_seconds: 125 }), 0, {});
  assert.equal(proving.kind, 'proving');
  assert.equal(proving.badge, 'Proving');
  assert.equal(proving.line, 'Proving (about 10 minutes)… 2:05');
  assert.equal(proving.elapsedSeconds, 125);
  assert.equal(proving.canCancel, true);
  assert.equal(proving.done, false);
  assert.equal(proving.provingStarted, true);

  const sending = describeStatus(appView('approved', { state: 'sending' }), 0, {});
  assert.equal(sending.kind, 'sending');
  assert.equal(sending.badge, 'Sending');
  assert.equal(sending.canCancel, false);
  assert.equal(sending.done, false);
  assert.equal(sending.provingStarted, true);

  const waitingBlock = describeStatus(appView('approved', { state: 'waiting-for-block' }), 0, {});
  assert.equal(waitingBlock.kind, 'sent');
  assert.equal(waitingBlock.badge, 'Sent');
  assert.equal(waitingBlock.canCancel, false);
  assert.equal(waitingBlock.done, false);
  assert.equal(waitingBlock.provingStarted, true);

  // Ready with a valid proof shape: NEVER "Valid" — must be neutral unverified state.
  const ready = describeStatus(
    appView('done', { state: 'ready', proof: validProof({ account: 'Private/must-drop', holder: 'Private/h' }) }),
    0,
    {},
  );
  assert.equal(ready.kind, 'unverified');
  assert.equal(ready.badge, 'Unchecked');
  assert.equal(ready.line, PROOF_UNCHECKED_LINE);
  assert.equal(ready.done, true);
  assert.equal(ready.isProblem, false);
  assert.equal(ready.proof.slot, VALID_HEX32);
  assert.equal(ready.proof.gate, VALID_ID_B);
  assert.equal(ready.proof.tx_hash, VALID_TX_HASH);
  assert.equal('account' in ready.proof, false);
  assert.equal('holder' in ready.proof, false);

  // Rejected and expired come ONLY on top-level `state`, while nested `status` still says awaiting-approval.
  const rejected = describeStatus(appView('rejected', { state: 'awaiting-approval' }), 0, {});
  assert.equal(rejected.kind, 'refused');
  assert.equal(rejected.done, true);
  assert.equal(rejected.code, 'REJECTED');
  assert.equal(rejected.line, WALLET_ERRORS.REJECTED);
  assert.match(rejected.line, /Nothing was sent\./);

  const expired = describeStatus(appView('expired', { state: 'awaiting-approval' }), 0, {});
  assert.equal(expired.kind, 'refused');
  assert.equal(expired.done, true);
  assert.equal(expired.code, 'EXPIRED');
  assert.equal(expired.line, WALLET_ERRORS.EXPIRED);
  assert.match(expired.line, /Nothing was sent\./);

  // Cancelled before proving (e.g. from cancel_membership_proof returning app_view).
  const cancelled = describeStatus(appView('cancelled', { state: 'cancelled' }), 0, { provingBegan: false });
  assert.equal(cancelled.kind, 'refused');
  assert.equal(cancelled.done, true);
  assert.equal(cancelled.code, 'CANCELLED');
  assert.equal(cancelled.line, CANCELLED_AFTER_PROVING_LINE);
  assert.ok(!/nothing was sent/i.test(cancelled.line));

  // Old nonexistent contract fields (`running`, `phase`, `error_code`) are NOT accepted.
  const oldRunning = describeStatus({ state: 'running', phase: 'prove', elapsed_seconds: 10 }, 0, {});
  assert.equal(oldRunning.statusReadFailed, true);
  assert.equal(oldRunning.done, false);

  const oldFailed = describeStatus({ state: 'failed', error_code: 'GATE_NOT_PINNED' }, 0, {});
  assert.equal(oldFailed.kind, 'refused');
  assert.equal(oldFailed.line, UNKNOWN_FAILURE_LINE);
});

test('Every WalletError code from excerpts.rs maps cleanly and only allowed codes say "Nothing was sent"', () => {
  const walletErrorFixtures = [
    ['NOT_A_MEMBER', true, 'This account holds no NFT of that collection, so there is nothing to prove. Nothing was sent.'],
    ['FEE_RESERVE', false, "The account paying the fee doesn't hold the fee reserve. Fund it first; that is one more private transaction."],
    ['ACCOUNT_BUSY', false, 'Your account changed while proving, or is busy with another transaction; try again.'],
    ['OUT_OF_WINDOW', false, 'The proof landed too late for its validity window; prove again.'],
    ['PROVER_OUT_OF_MEMORY', false, "This computer doesn't have enough memory to make the proof (a 16 GB machine is needed)."],
    ['RPC_UNREACHABLE', true, 'Could not reach the chain. Nothing was sent.'],
    ['GATE_NOT_PINNED', true, "The membership gate on this network isn't the pinned, unchangeable one (or none is set up yet). Nothing was sent."],
    ['UNEXPECTED_OUTCOME', true, 'The proven transaction would do something other than write one pass and pay the fee. It was stopped and not sent.'],
  ];

  for (const [code, maySayNothingSent, walletPlain] of walletErrorFixtures) {
    const rec = appView('failed', { state: 'failed', code, plain_message: walletPlain });

    // Before proving began: exact code is translated cleanly, raw code never leaked.
    const before = describeStatus(rec, 0, { provingBegan: false });
    assert.equal(before.kind, 'refused');
    assert.equal(before.done, true);
    assert.equal(before.code, code);
    assert.ok(!before.line.includes(code), `raw code ${code} leaked`);
    assert.equal(isProblem(before.line), true, `isProblem should be true for ${code}: ${before.line}`);
    assert.equal(/nothing was sent/i.test(before.line), maySayNothingSent, `${code} nothing-was-sent mismatch: ${before.line}`);

    // After proving began: ANY failure must say UNKNOWN_FAILURE_LINE and NEVER say "Nothing was sent".
    const after = describeStatus(rec, 0, { provingBegan: true });
    assert.equal(after.kind, 'refused');
    assert.equal(after.done, true);
    assert.equal(after.line, UNKNOWN_FAILURE_LINE);
    assert.ok(!/nothing was sent/i.test(after.line));
    assert.equal(isProblem(after.line), true);
  }
});

test('Unknown errors never guess by substrings; on-chain maps ONLY the real wallet refusal', () => {
  // Unknown code in status.failed stays unknown and never says "Nothing was sent".
  const unknownCode = describeStatus(
    appView('failed', { state: 'failed', code: 'FUTURE_ERROR', plain_message: 'something unexpected' }),
    0,
    { provingBegan: false },
  );
  assert.equal(unknownCode.line, UNKNOWN_FAILURE_LINE);
  assert.ok(!/nothing was sent/i.test(unknownCode.line));

  // Substrings like "rejected" or "rpc" or "not pinned" or "expired" must NOT be guessed.
  for (const text of [
    'sequencer rejected transaction',
    'rpc timeout while confirming',
    'not pinned in local cache',
    'fee reserve estimate failed',
    'session expired on node',
    'user cancelled? maybe',
    'nullifier conflict',
    'out of memory in browser',
  ]) {
    assert.equal(plainError('', text), UNKNOWN_FAILURE_LINE, `must not guess from substring: ${text}`);
  }

  // On-chain refusal: ONLY the exact Names 0.18.3 refusal maps to ONCHAIN_UNSUPPORTED.
  const realRefusal = "on-chain use isn't built in this wallet yet: ask for an off-chain proof (no `onchain`)";
  assert.equal(onchainRefusalMessage('', realRefusal), WALLET_ERRORS.ONCHAIN_UNSUPPORTED);
  assert.equal(onchainRefusalMessage('', realRefusal, { beforeId: true }), WALLET_ERRORS.ONCHAIN_UNSUPPORTED);
  assert.equal(plainError('', realRefusal), WALLET_ERRORS.ONCHAIN_UNSUPPORTED);

  // Unknown on-chain errors stay unknown (NOT ONCHAIN_UNSUPPORTED).
  assert.equal(onchainRefusalMessage('', 'unknown field onchain'), UNKNOWN_FAILURE_LINE);
  assert.equal(onchainRefusalMessage('', 'refused'), UNKNOWN_FAILURE_LINE);
  assert.equal(onchainRefusalMessage('', 'something else', { beforeId: true }), REQUEST_REFUSED_BEFORE_ID_LINE);

  // Real unlisted NFT program refusal before getting an id.
  const unlisted = plainError('', "that NFT program isn't in this network's list: add it under Collectibles, Programs", { beforeId: true });
  assert.match(unlisted, /Collectibles, Programs/);
  assert.match(unlisted, /Nothing was sent\./);
});

test('Cancel during proving warns that proving may still send (via status or cancel_membership_proof)', () => {
  let session = createRequestSession();
  session = applyPollSuccess(session, appView('pending', { state: 'awaiting-approval' }), 0, false);
  assert.equal(session.provingBegan, false);
  assert.equal(session.polling, true);

  session = applyPollSuccess(session, appView('approved', { state: 'checking' }), 0, false);
  assert.equal(session.provingBegan, false);

  session = applyPollSuccess(session, appView('approved', { state: 'proving', elapsed_seconds: 42 }), 0, false);
  assert.equal(session.provingBegan, true);
  assert.equal(session.polling, true);
  assert.equal(session.canCancel, true);
  assert.equal(session.actionsEnabled, false);

  // Cancel arrives from cancel_membership_proof (same app_view shape).
  const afterCancel = applyPollSuccess(session, appView('cancelled', { state: 'cancelled' }), 0, false);
  assert.equal(afterCancel.polling, false);
  assert.equal(afterCancel.actionsEnabled, true);
  assert.equal(afterCancel.statusKind, 'refused');
  assert.equal(afterCancel.stage, CANCELLED_AFTER_PROVING_LINE);
  assert.ok(!/nothing was sent/i.test(afterCancel.stage));
  assert.equal(isProblem(afterCancel.stage), true);
});

test('A failed status read is NOT a failed proof: keeps polling and never re-enables actions', () => {
  let session = createRequestSession();
  session = applyPollSuccess(session, appView('approved', { state: 'proving', elapsed_seconds: 90 }), 0, false);
  assert.equal(session.provingBegan, true);
  assert.equal(session.polling, true);
  assert.equal(session.actionsEnabled, false);

  for (const err of [
    { ok: false, busy: true, error: 'Names Wallet is busy.' },
    { ok: false, missing: true, error: 'Names Wallet is not open.' },
    { ok: false, unclear: true, error: 'No answer from Names Wallet yet.' },
    { ok: false, error: 'transport timeout' },
  ]) {
    const failedRead = applyPollFailure(session, err);
    assert.equal(failedRead.polling, true, 'must keep polling on status read failure');
    assert.equal(failedRead.actionsEnabled, false, 'must never re-enable actions on status read failure');
    assert.equal(failedRead.provingBegan, true, 'must preserve provingBegan');
    assert.equal(failedRead.statusKind, 'proving', 'must not mark proof refused');
    assert.ok(failedRead.statusReadWarning.length > 10);
    assert.equal(isProblem(failedRead.statusReadWarning), true);
  }

  // Unrecognized status payload is also treated as unreadable status, not a failed proof.
  const corrupt = applyPollSuccess(session, { unexpected: true }, 0, false);
  assert.equal(corrupt.polling, true);
  assert.equal(corrupt.actionsEnabled, false);
  assert.equal(corrupt.provingBegan, true);
  assert.equal(corrupt.statusKind, 'proving');
  assert.equal(corrupt.statusReadWarning, statusReadErrorLine(null));

  // Next successful poll clears the warning and advances normally.
  const recovered = applyPollSuccess(corrupt, appView('approved', { state: 'waiting-for-block' }), 0, false);
  assert.equal(recovered.polling, true);
  assert.equal(recovered.statusReadWarning, '');
  assert.equal(recovered.statusKind, 'sent');
  assert.equal(recovered.actionsEnabled, false);
});

test('Proof shape validation checks gate, slot, tx_hash, act_tx_hash and window', () => {
  assert.equal(validateProofShape(validProof()).ok, true);
  assert.equal(validateProofShape(validProof({ act_tx_hash: VALID_ACT_TX_HASH })).ok, true);

  const badProofs = [
    ['missing gate', validProof({ gate: '' })],
    ['bad gate b58', validProof({ gate: 'not-base58!' })],
    ['bad slot', validProof({ slot: 'abcd' })],
    ['uppercase slot', validProof({ slot: 'AB'.repeat(32) })],
    ['missing tx_hash', validProof({ tx_hash: '' })],
    ['bad tx_hash', validProof({ tx_hash: 'not-a-hash' })],
    ['bad act_tx_hash', validProof({ act_tx_hash: 'short' })],
    ['unquantized not_before', validProof({ not_before: VALID_NOT_BEFORE + 1 })],
    ['zero not_before', validProof({ not_before: 0 })],
    ['bad ttl window', validProof({ expires_at: VALID_NOT_BEFORE + 12345 })],
    ['expires before not_before', validProof({ expires_at: VALID_NOT_BEFORE - 3600000 })],
  ];

  for (const [label, bad] of badProofs) {
    assert.equal(validateProofShape(bad).ok, false, `expected invalid: ${label}`);
    const view = describeStatus(appView('done', { state: 'ready', proof: bad }), 0, {});
    assert.equal(view.kind, 'refused', `expected refused for ${label}`);
    assert.equal(view.code, 'MALFORMED', `expected MALFORMED for ${label}`);
    assert.equal(view.proof, null);
  }
});

test('No input at all can produce "Valid" in this build', () => {
  const candidates = [
    appView('pending', { state: 'awaiting-approval' }),
    appView('approved', { state: 'checking' }),
    appView('approved', { state: 'proving', elapsed_seconds: 5 }),
    appView('approved', { state: 'sending' }),
    appView('approved', { state: 'waiting-for-block' }),
    appView('done', { state: 'ready', proof: validProof() }),
    appView('done', { state: 'ready', proof: validProof({ act_tx_hash: VALID_ACT_TX_HASH }) }),
    appView('done', { state: 'valid', proof: validProof() }),
    appView('valid', { state: 'valid', proof: validProof() }),
    appView('failed', { state: 'failed', code: 'GATE_NOT_PINNED', plain_message: 'Valid' }),
    appView('rejected', { state: 'awaiting-approval' }),
    appView('expired', { state: 'awaiting-approval' }),
    appView('cancelled', { state: 'cancelled' }),
    { state: 'done', proof: validProof() },
    { state: 'OK', status: { state: 'OK' } },
    null,
    {},
  ];

  for (const rec of candidates) {
    for (const provingBegan of [false, true]) {
      const view = describeStatus(rec, 0, { provingBegan });
      assert.notEqual(view.kind, 'valid');
      assert.ok(!/\bvalid\b/i.test(view.badge), `badge must never say Valid: ${view.badge}`);
      assert.ok(!/\bvalid\b/i.test(view.line), `line must never say Valid: ${view.line}`);
    }
  }

  for (const p of [validProof(), validProof({ act_tx_hash: VALID_ACT_TX_HASH }), { slot: 'bad' }]) {
    const pass = describePass(p, 'Door Pass');
    assert.equal(pass.verified, false);
    assert.ok(!/\bvalid\b/i.test(pass.headline), `headline must never say Valid: ${pass.headline}`);
    assert.ok(!/\bvalid\b/i.test(pass.statusLine), `statusLine must never say Valid: ${pass.statusLine}`);
    assert.equal('establishes' in pass, false, 'describePass must not claim Establishes: holder of');
  }
});

test('Request contract: program is required and purpose is at most 120 characters', () => {
  assert.equal(MAX_PURPOSE, 120);

  const off = buildProofRequest({
    program: VALID_ID_A,
    collection: VALID_ID_C,
    purpose: 'Check in this app',
    nonce: VALID_HEX32,
    ttl: 3600000,
    onchain: false,
  });
  assert.equal(off.ok, true);
  assert.deepEqual(Object.keys(off.body).sort(), ['collection', 'nonce', 'program', 'purpose', 'ttl']);
  assert.equal(off.body.program, VALID_ID_A);

  const on = buildProofRequest({
    program: VALID_ID_A,
    collection: VALID_ID_C,
    purpose: "Sign the members' book",
    nonce: VALID_HEX32,
    ttl: 3600000,
    onchain: true,
    consumer: VALID_ID_B,
  });
  assert.equal(on.ok, true);
  assert.deepEqual(Object.keys(on.body).sort(), ['collection', 'nonce', 'onchain', 'program', 'purpose', 'ttl']);
  assert.deepEqual(on.body.onchain, { consumer: VALID_ID_B });

  // Missing or invalid program is refused plainly.
  const noProg = buildProofRequest({ collection: VALID_ID_C, purpose: 'Check in', nonce: VALID_HEX32, ttl: 3600000 });
  assert.equal(noProg.ok, false);
  assert.match(noProg.error, /NFT program/i);
  assert.equal(buildProofRequest({ program: 'not-base58!', collection: VALID_ID_C, purpose: 'Check in', nonce: VALID_HEX32, ttl: 3600000 }).ok, false);

  // Purpose: 120 chars passes, 121 chars fails, control characters fail.
  assert.equal(buildProofRequest({ program: VALID_ID_A, collection: VALID_ID_C, purpose: 'a'.repeat(120), nonce: VALID_HEX32, ttl: 3600000 }).ok, true);
  assert.equal(buildProofRequest({ program: VALID_ID_A, collection: VALID_ID_C, purpose: 'a'.repeat(121), nonce: VALID_HEX32, ttl: 3600000 }).ok, false);
  assert.equal(buildProofRequest({ program: VALID_ID_A, collection: VALID_ID_C, purpose: 'bad\nline', nonce: VALID_HEX32, ttl: 3600000 }).ok, false);

  // Other invalid fields refused.
  assert.equal(buildProofRequest({ program: VALID_ID_A, collection: 'bad', purpose: 'x', nonce: VALID_HEX32, ttl: 3600000 }).ok, false);
  assert.equal(buildProofRequest({ program: VALID_ID_A, collection: VALID_ID_C, purpose: '', nonce: VALID_HEX32, ttl: 3600000 }).ok, false);
  assert.equal(buildProofRequest({ program: VALID_ID_A, collection: VALID_ID_C, purpose: 'x', nonce: 'zz', ttl: 3600000 }).ok, false);
  assert.equal(buildProofRequest({ program: VALID_ID_A, collection: VALID_ID_C, purpose: 'x', nonce: VALID_HEX32, ttl: 90000000 }).ok, false);
  assert.equal(buildProofRequest({ program: VALID_ID_A, collection: VALID_ID_C, purpose: 'x', nonce: VALID_HEX32, ttl: 1.5 }).ok, false);
  assert.equal(buildProofRequest({ program: VALID_ID_A, collection: VALID_ID_C, purpose: 'x', nonce: VALID_HEX32, ttl: 3600000, onchain: true, consumer: '' }).ok, false);
});

test('Unknown zone never falls back to rc3; missing channel matches exact RPC only', () => {
  assert.equal(DEFAULT_NETWORK, 'rc3');
  assert.equal(NETWORKS.rc3.testnet, true);
  assert.equal(NETWORKS.testnet04.testnet, true);
  assert.equal(isBase58Id(NETWORKS.rc3.nftProgram), true);
  assert.equal(isBase58Id('not-valid!'), false);

  // rc3 has missing channel in config -> matches by exact RPC only.
  assert.equal(networkFor({ zone: { rpc: 'https://lez.nameswallet.app' } }).id, 'rc3');
  assert.equal(networkFor({ zone: { rpc: 'https://lez.nameswallet.app/' } }).id, 'rc3');
  assert.equal(networkFor({ zone: { channel: '11'.repeat(32), rpc: 'https://lez.nameswallet.app' } }).id, 'rc3');

  // Unknown zones return null (NEVER fall back to rc3).
  assert.equal(networkFor({ zone: { rpc: 'https://lez.nameswallet.app/other' } }), null);
  assert.equal(networkFor({ zone: { rpc: 'http://127.0.0.1:3310' } }), null);
  assert.equal(networkFor({ zone: { channel: 'ff'.repeat(32), rpc: 'https://unknown.example' } }), null);
  assert.equal(networkFor({ zone: {} }), null);
  assert.equal(networkFor(null), null);

  assert.equal(describeUnknownZone({ zone: { name: 'Custom Zone', rpc: 'https://x.example' } }), 'Custom Zone');
  assert.equal(describeUnknownZone({ zone: { rpc: 'https://x.example' } }), 'https://x.example');

  // Unknown network disables both actions even if collection and consumer are valid.
  const unknownResolved = resolveConfig(null, { collection: VALID_ID_C, consumer: VALID_ID_B });
  assert.equal(unknownResolved.known, false);
  assert.equal(unknownResolved.nftProgramValid, false);
  assert.equal(unknownResolved.readyOffchain, false);
  assert.equal(unknownResolved.readyOnchain, false);

  // Known network without nftProgram (testnet04 today) also disables both actions.
  const noNftProg = resolveConfig(NETWORKS.testnet04, { collection: VALID_ID_C, consumer: VALID_ID_B });
  assert.equal(noNftProg.known, true);
  assert.equal(noNftProg.nftProgramValid, false);
  assert.equal(noNftProg.readyOffchain, false);
  assert.equal(noNftProg.readyOnchain, false);

  // Known network with nftProgram (rc3).
  const emptyRc3 = resolveConfig(NETWORKS.rc3, {});
  assert.equal(emptyRc3.known, true);
  assert.equal(emptyRc3.nftProgramValid, true);
  assert.equal(emptyRc3.readyOffchain, false);
  assert.equal(emptyRc3.readyOnchain, false);

  const offOnly = resolveConfig(NETWORKS.rc3, { collection: VALID_ID_C });
  assert.equal(offOnly.readyOffchain, true);
  assert.equal(offOnly.readyOnchain, false);

  const both = resolveConfig(NETWORKS.rc3, { collection: VALID_ID_C, consumer: VALID_ID_B });
  assert.equal(both.readyOffchain, true);
  assert.equal(both.readyOnchain, true);
});

test('Progress wording, pass card, guide, sanitizeProof, formatters and PendingGuard', () => {
  assert.equal(qmlProgress('proving', 125), webProgress({ status: 'proving' }, 125));
  assert.equal(qmlProgress('awaiting-approval', 0), 'Waiting for you to approve in Names Wallet…');
  assert.equal(qmlProgress('checking', 0), 'Checking your NFT…');
  assert.equal(qmlProgress('sending', 0), 'Sending…');
  assert.equal(qmlProgress('waiting-for-block', 0), 'Sent; waiting for the block…');
  assert.equal(qmlProgress('ready', 0), PROOF_UNCHECKED_LINE);

  const line = proofLine(validProof());
  assert.match(line, /Proof sent\. Not checked on chain by this app yet\./);
  assert.match(line, /Pass id abababab…/);
  assert.ok(!line.includes('Public/') && !line.includes('Private/'));
  assert.ok(!line.includes(VALID_TX_HASH));

  const pass = describePass(validProof({ act_tx_hash: VALID_ACT_TX_HASH }), 'Door Pass');
  assert.equal(pass.shapeValid, true);
  assert.equal(pass.verified, false);
  assert.equal(pass.passIdShort, 'abababab…');
  assert.match(pass.windowLine, /2025-11-18 21:10 UTC to 2025-11-18 22:10 UTC/);
  assert.match(pass.onchainLine, /efefefef…/);
  assert.match(pass.reveals, /acting account/i);
  assert.match(pass.doesNotReveal, /Which NFT.*which account/i);
  assert.match(pass.trustNote, /has not checked this pass on chain/);

  const res = sanitizeProof(validProof({ account: 'Private/secret', holding: 'h1', balance: '999', commitment: '00'.repeat(32), nullifier: '11'.repeat(32) }));
  assert.deepEqual(Object.keys(res.proof).sort(), ['expires_at', 'gate', 'not_before', 'slot', 'tx_hash']);
  assert.deepEqual(res.dropped.sort(), ['account', 'balance', 'commitment', 'holding', 'nullifier']);

  assert.equal(doorStep({ proofReturned: false, waiting: false, busy: false }), 1);
  assert.equal(doorStep({ proofReturned: false, waiting: true, busy: false }), 2);
  assert.equal(doorStep({ proofReturned: true, waiting: false, busy: false }), 3);
  assert.equal(doorSteps(2)[1].state, 'now');

  assert.equal(formatElapsed(0), '0:00');
  assert.equal(formatElapsed(-5), '0:00');
  assert.equal(formatElapsed(65), '1:05');
  assert.equal(formatUtcTime(VALID_EXPIRES_AT), '2025-11-18 22:10 UTC');
  assert.equal(formatUtcTime(0), '');
  assert.equal(buttonBusyLabel('preparing', 0), 'Preparing…');
  assert.equal(buttonBusyLabel('waiting', 0), 'Waiting in Names…');
  assert.equal(buttonBusyLabel('checking', 5), 'Checking…');
  assert.equal(buttonBusyLabel('proving', 125), 'Proving… 2:05');
  assert.equal(buttonBusyLabel('sending', 3), 'Sending…');
  assert.equal(buttonBusyLabel('sent', 3), 'Waiting for block…');

  assert.equal(shortId(''), '');
  assert.equal(shortId('twelvechars!'), 'twelvechars!');
  assert.equal(shortId('abcdefghijklmnop'), 'abcdefgh…');

  assert.equal(isProblem(PROOF_UNCHECKED_LINE), false);
  assert.equal(isProblem('Names Wallet is ready.'), false);
  assert.equal(isProblem(UNKNOWN_FAILURE_LINE), true);
  assert.equal(isProblem(CANCELLED_AFTER_PROVING_LINE), true);

  const guard = new PendingGuard();
  assert.equal(guard.take('alice'), true);
  assert.equal(guard.take('alice'), false);
  guard.release('alice');
  assert.equal(guard.take('alice'), true);
});

// LP-0001 privacy: the app proves membership without ever asking for account access.
test("Members' door never asks the wallet for account access", () => {
  const qml = readFileSync(new URL('../basecamp/qml/Main.qml', import.meta.url), 'utf8');
  for (const forbidden of [
    'wallet.connect',
    'wallet.check',
    'wallet.balances',
    'request_access',
    'granted_accounts',
    'account_balances',
    'request_nft_access',
    'granted_nfts',
  ]) {
    assert.ok(!qml.includes(forbidden), `${forbidden} must not appear in Members' door Main.qml`);
  }
});

function extractTextBlocks(source) {
  const blocks = [];
  const re = /\bText\s*\{/g;
  let m;
  while ((m = re.exec(source)) !== null) {
    let depth = 1;
    let i = m.index + m[0].length;
    while (i < source.length && depth > 0) {
      if (source[i] === '{') depth++;
      else if (source[i] === '}') depth--;
      i++;
    }
    blocks.push(source.slice(m.index, i));
  }
  return blocks;
}

function stripComments(source) {
  return source
    .replace(/\/\*[\s\S]*?\*\//g, '')
    .replace(/(^|\s)\/\/.*$/gm, '$1');
}

test('Basecamp QML, UI honesty and packaging rules on Main.qml, logic.mjs, networks.mjs, metadata.json and flake.nix', () => {
  const mainQml = readFileSync(new URL('../basecamp/qml/Main.qml', import.meta.url), 'utf8');
  const logicMjs = readFileSync(new URL('../basecamp/qml/logic.mjs', import.meta.url), 'utf8');
  const networksMjs = readFileSync(new URL('../basecamp/qml/networks.mjs', import.meta.url), 'utf8');
  const metaRaw = readFileSync(new URL('../basecamp/metadata.json', import.meta.url), 'utf8');
  const flakeNix = readFileSync(new URL('../basecamp/flake.nix', import.meta.url), 'utf8');
  const iconSvg = readFileSync(new URL('../basecamp/src/icons/icon.svg', import.meta.url), 'utf8');

  const javaReservedDecl = /\b(?:function|var|property\s+\w+)\s+(?:short|int|char|long|byte|float|double|final|abstract|boolean|goto|native|synchronized|throws|transient|volatile|package)\b/;

  for (const [name, src] of [
    ['qml/Main.qml', mainQml],
    ['qml/logic.mjs', logicMjs],
    ['qml/networks.mjs', networksMjs],
  ]) {
    const code = stripComments(src);
    assert.ok(!/\bBigInt\b/.test(code), `${name} must not use BigInt`);
    assert.ok(!/\b\d+n\b/.test(code), `${name} must not use BigInt literals`);
    assert.ok(!javaReservedDecl.test(code), `${name} must not declare Java reserved word names`);
    assert.ok(!/\bofficial\b/i.test(src), `${name} must not use the word "official"`);
  }

  // No hard-coded base58 program/collection IDs in app logic (they belong only in networks.mjs).
  for (const [name, src] of [['qml/Main.qml', mainQml], ['qml/logic.mjs', logicMjs]]) {
    assert.ok(!/"[1-9A-HJ-NP-Za-km-z]{32,44}"/.test(src), `${name} must not hard-code a base58 ID`);
  }

  // Left-aligned column next to Basecamp's menu, never centred.
  assert.ok(mainQml.includes('look.edge') && mainQml.includes('look.pageWidth'));
  assert.ok(!mainQml.includes('anchors.horizontalCenter'));

  // Never show green / "Valid" / "Establishes" while unchecked.
  const mainCode = stripComments(mainQml);
  assert.ok(!mainCode.includes('look.good'), 'Main.qml must not use green (look.good) for unchecked passes');
  assert.ok(!/"Valid"/.test(mainCode), 'Main.qml must not show "Valid"');
  assert.ok(!/"Establishes"/.test(mainCode), 'Main.qml must not show "Establishes"');
  assert.ok(!mainCode.includes('statusKind === "valid"'), 'Main.qml must not have a "valid" statusKind');
  assert.ok(!/replay protection/i.test(mainCode), 'Main.qml must not claim replay protection on screen');

  // Exactly one collection input field in Main.qml (no duplicate collectionOverride fields).
  const collectionFieldMatches = mainQml.match(/text:\s*root\.collectionOverride/g) || [];
  assert.equal(collectionFieldMatches.length, 1, 'Main.qml must have exactly one collection field');

  // Main.qml must not use the nonexistent `phase` or `error_code` fields.
  assert.ok(!mainCode.includes('record.phase'));
  assert.ok(!mainCode.includes('error_code'));

  // Every Text in Main.qml and every kit component sets textFormat: Text.PlainText.
  const kitFiles = [
    'Action.qml', 'BackButton.qml', 'FactRow.qml', 'Field.qml',
    'Link.qml', 'Look.qml', 'PageHeader.qml', 'RunsOn.qml', 'TechDetails.qml',
  ];
  for (const [name, src] of [
    ['qml/Main.qml', mainQml],
    ...kitFiles.map((f) => [`qml/kit/${f}`, readFileSync(new URL(`../basecamp/qml/kit/${f}`, import.meta.url), 'utf8')]),
  ]) {
    for (const block of extractTextBlocks(src)) {
      assert.ok(/textFormat\s*:\s*Text\.PlainText/.test(block), `${name} has a Text without textFormat: Text.PlainText`);
    }
  }

  // Packaging checks: names >=0.18.0 dependency and flake input restored.
  assert.ok(!/\bofficial\b/i.test(metaRaw));
  assert.ok(!/\bofficial\b/i.test(flakeNix));
  const meta = JSON.parse(metaRaw);
  assert.equal(meta.author, 'lns:names');
  assert.equal(meta.type, 'ui_qml');
  assert.equal(meta.view, 'qml/Main.qml');
  assert.equal(meta.icon, 'src/icons/icon.svg');
  assert.deepEqual(meta.dependencies, [{ name: 'names', version: '>=0.18.0' }]);
  assert.ok(meta.uses.some((u) => u.intent === 'basecamp.apps.launch'));
  assert.ok(flakeNix.includes('mkLogosQmlModule'));
  assert.ok(flakeNix.includes('configFile = ./metadata.json'));
  assert.ok(flakeNix.includes('names.url = "path:../names"'));
  assert.ok(flakeNix.includes('names.inputs.logos-module-builder.follows = "logos-module-builder"'));
  assert.ok(iconSvg.includes('<svg') && iconSvg.includes('</svg>'));
});
