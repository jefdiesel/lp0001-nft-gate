// UX regression tests: the text helpers the QML view uses must stay plain and
// must not leak an account, NFT or raw state code. No Qt runner is needed.
import test from 'node:test';
import assert from 'node:assert/strict';

import { progressLine as qmlProgress, proofLine, doorStep, doorSteps, isProblem, short } from '../basecamp/src/qml/logic.mjs';
import { progressLine as webProgress, PendingGuard } from '../web/logic.mjs';

test('QML progress uses plain words, never a raw state code', () => {
  assert.equal(qmlProgress('pending'), 'Waiting for you to approve in the wallet…');
  assert.equal(qmlProgress('running', 'prove', 125), 'Proving (about 10 minutes)… 2:05');
  assert.equal(qmlProgress('running', 'simulate', 5), 'Checking your NFT… 0:05');
  assert.equal(qmlProgress('done'), 'Ready.');
  assert.equal(qmlProgress('rejected'), 'You declined the request. Nothing was sent.');
  for (const state of ['awaiting-approval', 'waiting-for-block', 'BINDING_MISMATCH']) {
    assert.ok(!qmlProgress(state).includes(state), `raw code leaked: ${state}`);
  }
});

test('progress wording matches the web page for the same stage', () => {
  assert.equal(
    qmlProgress('running', 'prove', 125),
    webProgress({ status: 'proving' }, 125),
  );
  assert.equal(qmlProgress('done'), webProgress({ status: 'ready' }, 0));
});

test('a finished proof shows a pass id but no NFT or account', () => {
  const line = proofLine({ slot: 'ab'.repeat(32), tx_hash: 'cd'.repeat(32) });
  assert.match(line, /Pass id abababab…/);
  assert.ok(!line.includes('Public/'));
  assert.ok(!line.includes('Private/'));
  assert.ok(!line.includes('cd'.repeat(32))); // the tx hash is not printed
  assert.equal(proofLine(null), 'Proof ready.');
  assert.equal(proofLine({ slot: 'short' }), 'Proof ready.');
});

test('one pending request per acting account', () => {
  const guard = new PendingGuard();
  assert.equal(guard.take('alice'), true);
  assert.equal(guard.take('alice'), false);
  assert.equal(guard.take('bob'), true);
  guard.release('alice');
  assert.equal(guard.take('alice'), true);
});

test('the door guide marks one step, with its help, from the view state', () => {
  const idle = { proofReady: false, waiting: false, busy: false };
  assert.equal(doorStep(idle), 1);
  assert.equal(doorStep(Object.assign({}, idle, { busy: true })), 2);
  assert.equal(doorStep(Object.assign({}, idle, { waiting: true })), 2);
  assert.equal(doorStep(Object.assign({}, idle, { proofReady: true })), 3);
  // A refused or withdrawn request stops the poll, so step 1 is current again.
  assert.equal(doorStep({ proofReady: false, waiting: false, busy: false }), 1);

  const rows = doorSteps(2);
  assert.equal(rows.length, 3);
  assert.deepEqual(rows.map((r) => r.n), [1, 2, 3]);
  assert.deepEqual(rows.map((r) => r.state), ['done', 'now', 'later']);
  assert.deepEqual(rows.map((r) => r.text), ['Ask for a proof', 'Approve in the wallet', 'Read your pass']);
  for (const row of rows) {
    assert.ok(row.detail.length > 20, `step ${row.n} needs help text`);
    assert.ok(!row.detail.includes('Public/') && !row.detail.includes('Private/'));
  }
  assert.equal(doorSteps(1)[0].state, 'now');
  assert.equal(doorSteps(3)[2].state, 'now');
});

test('only a real problem is red; short ids keep the full value out of a heading', () => {
  assert.equal(isProblem('Preparing…'), false);
  assert.equal(isProblem('Waiting for you to approve in the wallet…'), false);
  assert.equal(isProblem('Proof ready. Pass id abababab…'), false);
  assert.equal(isProblem('Not done: the wallet refused the request.'), true);
  assert.equal(isProblem('The wallet is busy. Try again in a moment.'), true);
  assert.equal(isProblem(''), false);
  assert.equal(short(''), '');
  assert.equal(short('short'), 'short');
  assert.equal(short('abcdefghijklmnop'), 'abcdefgh…');
});
