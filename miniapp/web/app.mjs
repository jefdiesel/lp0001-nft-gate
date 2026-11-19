// SPDX-License-Identifier: MIT OR Apache-2.0
// Members' door, the static page. Static files only: no server of ours, no Worker. Reads go
// from the browser straight to the LEZ RPC the user names (which must allow the page's origin;
// a local sequencer may need a CORS-allowing proxy of the user's own).
import { b58decode32, unhex } from '../../js/gate-verify/src/core.mjs';
import { verifyOffchain, verifyOnchainUse, rpcCall } from '../../js/gate-verify/src/rpc.mjs';
import { createMembershipClient } from '../../js/wallet-shim/src/membership.mjs';
import { describeVerdict, progressLine, PendingGuard, gatedFromAct, newNonce, parseProofObject } from './logic.mjs';

const $ = (id) => document.getElementById(id);
const enc = new TextEncoder();
const pending = new PendingGuard();

function config() {
  const c = JSON.parse($('config').value || '{}');
  if (!c.gate || !c.nft_program || !c.collection || !c.zone_id) throw new Error('paste the deployment config first');
  return {
    raw: c,
    pinned: { gate: b58decode32(c.gate), nftProgram: b58decode32(c.nft_program), collection: b58decode32(c.collection), zoneId: unhex(c.zone_id) },
  };
}

function show(el, text, ok) {
  el.hidden = false;
  el.className = `result ${ok ? 'ok' : 'bad'}`;
  el.innerHTML = text;
}

/** In Basecamp, the wallet module; in a plain browser there is none. */
function walletTransport() {
  const logos = globalThis.logos;
  if (!logos?.callModuleAsync) return null;
  return { call: (method, ...args) => new Promise((ok) => logos.callModuleAsync('names', method, args, ok)) };
}

async function request(onchain) {
  const btn = onchain ? $('btn-on') : $('btn-off');
  btn.disabled = true;
  $('progress').textContent = 'Preparing…';
  try {
    const { raw } = config();
    const transport = walletTransport();
    if (!transport) throw new Error('No wallet here: open this app in Basecamp with the Names Wallet, or paste a proof object below.');
    const key = onchain ? 'onchain' : 'offchain';
    if (!pending.take(key)) throw new Error('A request is already waiting; finish or cancel it first.');
    const nonce = newNonce();
    $('nonce').value = nonce;
    const mp = createMembershipClient(transport, { isVisible: () => document.visibilityState === 'visible' });
    const { requestId } = await mp.requestMembershipProof({
      collection: raw.collection, purpose: $('purpose').value, nonce, ttl: 3_600_000,
      ...(onchain ? { onchain: { consumer: raw.consumer } } : {}),
    });
    const started = Date.now();
    mp.onProofStatus(requestId, (s) => {
      $('progress').textContent = progressLine(s, Math.floor((Date.now() - started) / 1000));
      if (s.status === 'ready') {
        $('proof').value = JSON.stringify(s.proof, null, 1);
        if (s.proof.act_tx_hash) $('acttx').value = s.proof.act_tx_hash;
      }
      if (s.status === 'ready' || s.status === 'failed') { pending.release(key); btn.disabled = false; }
    });
  } catch (e) {
    $('progress').textContent = `Not done: ${e.message}`;
    btn.disabled = false;
  }
}

async function verifyHere() {
  const btn = $('btn-verify');
  btn.disabled = true;
  show($('verdict'), 'Checking…', true);
  try {
    const { pinned, raw } = config();
    const proof = parseProofObject($('proof').value);
    const v = await verifyOffchain({
      rpcUrl: $('rpc').value, pinned, proof,
      use: { callerIdentity: enc.encode($('caller').value), purpose: enc.encode($('purpose').value), nonce: unhex($('nonce').value) },
    });
    const d = describeVerdict(v, { collectionName: raw.collection_name ?? raw.collection, rpc: $('rpc').value });
    show($('verdict'), `<strong>${d.headline}</strong><br>Establishes: ${d.establishes}<br>Does not reveal: ${d.doesNotReveal}<br><small>${d.trust}</small>`, d.ok);
    // Spec §2.8.5: the nonce is used once. A real app marks it used in its own session store.
  } catch (e) {
    show($('verdict'), `Could not check: ${e.message}`, false);
  } finally {
    btn.disabled = false;
  }
}

async function gated() {
  const btn = $('btn-gated');
  btn.disabled = true;
  show($('gated'), 'Reading…', true);
  try {
    const { pinned, raw } = config();
    const rpc = $('rpc').value;
    const tx = await rpcCall(rpc, 'getTransaction', [$('acttx').value.trim()]);
    const act = gatedFromAct(tx);
    const block = await rpcCall(rpc, 'getBlock', [act.blockId]);
    const v = await verifyOnchainUse({
      rpcUrl: rpc, pinned, proof: { gate: pinned.gate, slot: unhex(act.slot) },
      consumer: b58decode32(raw.consumer), submissionId: unhex(raw.submission_id ?? '00'.repeat(32)),
      actor: b58decode32(act.actor), nonce: BigInt($('actnonce').value || '0'),
    });
    show($('gated'), `<strong>Gated action</strong> by <code>${act.actor}</code> in block ${act.blockId}<br>
      submission <code>${raw.submission_id}</code>, pass <code>${act.slot}</code><br>
      Same pass checked in this page's code: <strong>${v.name}</strong><br>
      <small>Whether the Act reverted is in the block's reverted list, not decoded here (see <code>gate-cli gated</code>).</small><br>
      <small>Block ${block ? 'read' : 'missing'} from ${rpc}, trusted for this answer.</small>`, v.name === 'OK');
  } catch (e) {
    show($('gated'), `Could not read it: ${e.message}`, false);
  } finally {
    btn.disabled = false;
  }
}

$('btn-off').onclick = () => request(false);
$('btn-on').onclick = () => request(true);
$('btn-verify').onclick = verifyHere;
$('btn-gated').onclick = gated;
$('config').oninput = () => { try { $('collname').textContent = JSON.parse($('config').value).collection_name ?? JSON.parse($('config').value).collection; } catch { /* typing */ } };
