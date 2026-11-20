// SPDX-License-Identifier: MIT OR Apache-2.0
use super::*;
use std::cell::RefCell;
use std::collections::HashMap;

/// A fake RPC answering the three methods from fixed JSON.
struct FakeRpc {
    accounts: HashMap<String, Value>,
    last_block: Option<u64>,
    block_b64: Option<String>,
    events: Vec<Value>,
    calls: RefCell<Vec<String>>,
}

impl RpcJson for FakeRpc {
    fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        self.calls.borrow_mut().push(method.to_owned());
        match method {
            "getAccountView" => {
                let id = params[0]["account_id"].as_str().unwrap().to_owned();
                Ok(self.accounts.get(&id).cloned().unwrap_or(Value::Null))
            }
            "getLastBlockId" => self.last_block.map(Value::from).ok_or_else(|| "connection refused".to_owned()),
            "getBlock" => Ok(self.block_b64.clone().map(Value::from).unwrap_or(Value::Null)),
            "getEvents" => Ok(Value::Array(self.events.clone())),
            _ => Err("no such method".to_owned()),
        }
    }
}

fn b64encode(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(A[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// A block's borsh bytes: header (id, prev hash, hash, timestamp), then an empty body and a
/// status byte. Only the header is read.
fn block_bytes(id: u64, timestamp: u64) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&id.to_le_bytes());
    b.extend_from_slice(&[0x11; 32]);
    b.extend_from_slice(&[0x22; 32]);
    b.extend_from_slice(&timestamp.to_le_bytes());
    b.extend_from_slice(&0_u32.to_le_bytes()); // transactions: empty vec
    b.extend_from_slice(&0_u32.to_le_bytes()); // reverted: empty vec
    b.push(0); // bedrock_status: Pending
    b
}

const NB: u64 = 1_763_500_200_000;

fn pinned() -> Pinned {
    Pinned { gate: [0x6A; 32], nft_program: [0xAA; 32], collection: [0xC0; 32], zone_id: [0x2E; 32] }
}

fn use_() -> OffchainUse<'static> {
    OffchainUse { caller_identity: b"basecamp:members-door", purpose: b"Check in this app", nonce: [7; 32] }
}

fn world(state: Option<Vec<u8>>, now: Option<u64>) -> (JsonRpcReader<FakeRpc>, ProofObject) {
    world_events(state, now, Vec::new())
}

fn world_events(
    state: Option<Vec<u8>>,
    now: Option<u64>,
    events: Vec<Value>,
) -> (JsonRpcReader<FakeRpc>, ProofObject) {
    let p = pinned();
    let secret_slot = [0x51; 32];
    let pda = gate_core::slot_pda::<Sha2>(&p.gate, &secret_slot);
    let mut accounts = HashMap::new();
    if let Some(s) = state {
        let mut states = serde_json::Map::new();
        states.insert(b58encode(&p.gate), Value::from(s));
        states.insert(b58encode(&[0; 32]), Value::from(vec![9_u8; 16])); // native balance
        accounts.insert(b58encode(&pda), json!({ "nonce": 0, "data": { "actor_states": states } }));
    }
    let rpc = FakeRpc {
        accounts,
        last_block: now.map(|_| 42),
        block_b64: now.map(|t| b64encode(&block_bytes(42, t))),
        events,
        calls: RefCell::new(Vec::new()),
    };
    let proof = ProofObject { gate: p.gate, slot: secret_slot, not_before: NB, expires_at: NB + 3_600_000, tx_hash: None };
    (JsonRpcReader(rpc), proof)
}

/// One LEZ indexer EventRecord for a Gated event, in the JSON the RPC returns.
fn gated_event(program: Bytes32, submission: Bytes32, slot: Bytes32, actor: Bytes32, nonce: u64) -> Value {
    gated_event_tx(program, [0x42; 32], submission, slot, actor, nonce)
}

fn gated_event_tx(
    program: Bytes32,
    tx: Bytes32,
    submission: Bytes32,
    slot: Bytes32,
    actor: Bytes32,
    nonce: u64,
) -> Value {
    let mut data = Vec::with_capacity(112);
    data.extend_from_slice(&submission);
    data.extend_from_slice(&slot);
    data.extend_from_slice(&actor);
    data.extend_from_slice(&nonce.to_le_bytes());
    data.extend_from_slice(&(NB + 3_600_000).to_le_bytes());
    json!({
        "block_id": 42,
        "tx_index": 0,
        "tx_hash": hex(&tx),
        "program_account_id": b58encode(&program),
        "selector": hex(&gated_selector()),
        "data": b64encode(&data),
    })
}

fn good_onchain_fact(context: Bytes32) -> Vec<u8> {
    let p = pinned();
    Fact {
        binding: gate_core::binding::<Sha2>(&p.nft_program, &p.collection, &context),
        not_before: NB,
        expires_at: NB + 3_600_000,
    }
    .encode()
    .to_vec()
}

fn good_fact() -> Vec<u8> {
    let p = pinned();
    let ctx = offchain_context(&p, &use_());
    Fact { binding: gate_core::binding::<Sha2>(&p.nft_program, &p.collection, &ctx), not_before: NB, expires_at: NB + 3_600_000 }
        .encode()
        .to_vec()
}

#[test]
fn a_current_fact_is_ok_and_only_reads() {
    let (reader, proof) = world(Some(good_fact()), Some(NB + 5));
    let v = verify_offchain(&reader, &pinned(), &proof, &use_());
    assert_eq!(v.outcome, Outcome::Ok, "{v:?}");
    assert_eq!(v.chain_time_ms, Some(NB + 5));
    assert_eq!(*reader.0.calls.borrow(), vec!["getAccountView", "getLastBlockId", "getBlock"]);
}

#[test]
fn time_codes_come_from_the_chain_time() {
    let (reader, proof) = world(Some(good_fact()), Some(NB + 3_600_000));
    assert_eq!(verify_offchain(&reader, &pinned(), &proof, &use_()).outcome, Outcome::Expired);
    let (reader, proof) = world(Some(good_fact()), Some(NB - 1));
    assert_eq!(verify_offchain(&reader, &pinned(), &proof, &use_()).outcome, Outcome::NotYetValid);
}

#[test]
fn another_nonce_or_purpose_or_caller_is_binding_mismatch() {
    let (reader, proof) = world(Some(good_fact()), Some(NB + 5));
    let p = pinned();
    for u in [
        OffchainUse { nonce: [8; 32], ..use_() },
        OffchainUse { purpose: b"Other", ..use_() },
        OffchainUse { caller_identity: b"basecamp:another-app", ..use_() },
    ] {
        assert_eq!(verify_offchain(&reader, &p, &proof, &u).outcome, Outcome::BindingMismatch);
    }
    let other_collection = Pinned { collection: [0xC1; 32], ..p };
    assert_eq!(verify_offchain(&reader, &other_collection, &proof, &use_()).outcome, Outcome::BindingMismatch);
}

#[test]
fn an_empty_slot_is_no_fact() {
    let (reader, proof) = world(None, Some(NB));
    assert_eq!(verify_offchain(&reader, &pinned(), &proof, &use_()).outcome, Outcome::NoFact);
}

#[test]
fn no_rpc_is_rpc_unavailable() {
    let (reader, proof) = world(Some(good_fact()), None);
    let v = verify_offchain(&reader, &pinned(), &proof, &use_());
    assert_eq!(v.outcome, Outcome::RpcUnavailable);
    assert!(v.error.is_some());
}

#[test]
fn a_proof_for_another_gate_is_refused_without_reading() {
    let (reader, mut proof) = world(Some(good_fact()), Some(NB));
    proof.gate = [0x6B; 32];
    assert_eq!(verify_offchain(&reader, &pinned(), &proof, &use_()).outcome, Outcome::BindingMismatch);
    assert!(reader.0.calls.borrow().is_empty());
}

#[test]
fn the_context_is_never_taken_from_the_proof_object() {
    // The proof object's times are not used: a fact the chain has is what is checked.
    let (reader, mut proof) = world(Some(good_fact()), Some(NB + 5));
    proof.not_before = 0;
    proof.expires_at = u64::MAX;
    assert_eq!(verify_offchain(&reader, &pinned(), &proof, &use_()).outcome, Outcome::Ok);
}

#[test]
fn proof_times_and_tx_hash_are_ignored_the_chain_fact_decides() {
    // Today's behaviour: the wallet's proof object carries times and a tx hash, but the
    // verifier reads only `gate` and `slot`. Times that disagree with the fact at the RPC
    // cannot change the verdict; the fact read from the chain decides it.
    let (reader, mut proof) = world(Some(good_fact()), Some(NB + 5));
    proof.not_before = NB + 3_600_000; // would be NOT_YET_VALID if the proof object were trusted
    proof.expires_at = NB + 1; // would be EXPIRED if the proof object were trusted
    proof.tx_hash = Some([0x99; 32]);
    let v = verify_offchain(&reader, &pinned(), &proof, &use_());
    assert_eq!(v.outcome, Outcome::Ok, "{v:?}");
    let fact = v.fact.expect("the verdict carries the chain fact");
    assert_eq!(fact.not_before, NB, "the fact's time, not the proof's");
    assert_eq!(fact.expires_at, NB + 3_600_000, "the fact's time, not the proof's");
    assert_eq!(*reader.0.calls.borrow(), vec!["getAccountView", "getLastBlockId", "getBlock"]);
}

#[test]
fn account_json_shape_is_lee_core_shape() {
    // The exact JSON lee_core's own test prints (account.rs, an_account_serializes_with_its_data_nested).
    let v: Value = serde_json::from_str(
        r#"{"nonce":7,"data":{"actor_states":{"11111111111111111111111111111111":[9,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]}}}"#,
    )
    .unwrap();
    assert_eq!(actor_state_from_account_json(&v, &[0; 32]).unwrap()[0], 9);
    assert!(actor_state_from_account_json(&v, &[1; 32]).unwrap().is_empty());
    assert!(actor_state_from_account_json(&Value::Null, &[0; 32]).unwrap().is_empty());
}

#[test]
fn base58_matches_the_vectors_and_the_chain() {
    let v: Value = serde_json::from_str(include_str!("../../../vectors/gate-vectors.json")).unwrap();
    for d in v["derivations"].as_array().unwrap().iter().filter(|d| d.get("pda_b58").is_some()) {
        let pda = unhex32(d["pda"].as_str().unwrap()).unwrap();
        assert_eq!(b58encode(&pda), d["pda_b58"].as_str().unwrap());
        assert_eq!(b58decode32(d["pda_b58"].as_str().unwrap()).unwrap(), pda);
    }
    let o = &v["observed_pda"];
    assert_eq!(b58decode32(o["program_b58"].as_str().unwrap()).unwrap(), unhex32(o["program"].as_str().unwrap()).unwrap());
    assert_eq!(b58encode(&unhex32(o["pda"].as_str().unwrap()).unwrap()), o["pda_b58"].as_str().unwrap());
    assert_eq!(b58encode(&[0; 32]), "11111111111111111111111111111111");
}

#[test]
fn base64_round_trips() {
    for n in 0..20 {
        let data: Vec<u8> = (0..n).map(|i| u8::try_from(i * 37 % 256).unwrap()).collect();
        assert_eq!(b64decode(&b64encode(&data)).unwrap(), data);
    }
    assert_eq!(block_timestamp_from_borsh(&block_bytes(1, 1_763_500_000_123)).unwrap(), 1_763_500_000_123);
}

fn onchain_case() -> (Pinned, Bytes32, Bytes32, Bytes32, Bytes32, Bytes32) {
    // pinned, consumer, submission id, tx hash, acting account, slot
    let p = pinned();
    let consumer = [0xCC; 32];
    let submission = [0x5B; 32];
    let tx_hash = [0x42; 32];
    let actor = [0xA1; 32];
    let slot = [0x51; 32];
    (p, consumer, submission, tx_hash, actor, slot)
}

#[test]
fn a_gated_event_from_the_consumer_supplies_actor_and_nonce() {
    let (pinned, consumer, submission, tx_hash, actor, slot) = onchain_case();
    let context = onchain_context(&pinned, &consumer, &submission, &actor, 0);
    let event = gated_event(consumer, submission, slot, actor, 0);
    let (reader, proof) = world_events(Some(good_onchain_fact(context)), Some(NB + 1), vec![event]);
    let verdict = verify_onchain_use(&reader, &pinned, &proof, &OnchainUse { consumer, submission_id: submission, tx_hash });
    assert_eq!(verdict.outcome, Outcome::Ok, "{verdict:?}");
    assert_eq!(*reader.0.calls.borrow(), vec!["getEvents", "getAccountView", "getLastBlockId", "getBlock"]);
}

#[test]
fn a_gated_event_from_another_program_is_refused() {
    let (pinned, consumer, submission, tx_hash, actor, slot) = onchain_case();
    let context = onchain_context(&pinned, &consumer, &submission, &actor, 0);
    let forged = gated_event([0x99; 32], submission, slot, actor, 0);
    let (reader, proof) = world_events(Some(good_onchain_fact(context)), Some(NB + 1), vec![forged]);
    let verdict = verify_onchain_use(&reader, &pinned, &proof, &OnchainUse { consumer, submission_id: submission, tx_hash });
    assert_eq!(verdict.outcome, Outcome::NoFact, "{verdict:?}");
}

#[test]
fn an_event_from_another_transaction_is_refused_even_with_the_right_emitter() {
    let (pinned, consumer, submission, tx_hash, actor, slot) = onchain_case();
    let context = onchain_context(&pinned, &consumer, &submission, &actor, 0);
    let other_tx = gated_event_tx(consumer, [0x43; 32], submission, slot, actor, 0);
    let (reader, proof) = world_events(Some(good_onchain_fact(context)), Some(NB + 1), vec![other_tx]);
    let verdict = verify_onchain_use(&reader, &pinned, &proof, &OnchainUse { consumer, submission_id: submission, tx_hash });
    assert_eq!(verdict.outcome, Outcome::NoFact, "{verdict:?}");
}

#[test]
fn a_forged_event_before_the_real_one_is_skipped() {
    let (pinned, consumer, submission, tx_hash, actor, slot) = onchain_case();
    let context = onchain_context(&pinned, &consumer, &submission, &actor, 0);
    let forged = gated_event([0x99; 32], submission, slot, [0xA2; 32], 9);
    let real = gated_event(consumer, submission, slot, actor, 0);
    let (reader, proof) = world_events(Some(good_onchain_fact(context)), Some(NB + 1), vec![forged, real]);
    let verdict = verify_onchain_use(&reader, &pinned, &proof, &OnchainUse { consumer, submission_id: submission, tx_hash });
    assert_eq!(verdict.outcome, Outcome::Ok, "{verdict:?}");
}

#[test]
fn a_shared_gated_event_vector_agrees_with_javascript() {
    let v: Value = serde_json::from_str(include_str!("../../../vectors/gated-event-vector.json")).unwrap();
    let unhexv = |k: &str| -> Bytes32 { unhex32(v[k].as_str().unwrap()).unwrap() };
    let pinned = Pinned {
        gate: unhexv("gate"),
        nft_program: unhexv("nft_program"),
        collection: unhexv("collection"),
        zone_id: unhexv("zone_id"),
    };
    let consumer = unhexv("consumer");
    let submission = unhexv("submission_id");
    let tx_hash = unhexv("tx_hash");
    let slot = unhexv("slot");
    let fact = v["fact"].as_array().unwrap();
    let state: Vec<u8> = fact.iter().map(|b| b.as_u64().unwrap() as u8).collect();
    let event: Value = serde_json::from_value(v["event"].clone()).unwrap();
    let (reader, proof) = world_events(Some(state), Some(v["now_ms"].as_u64().unwrap()), vec![event]);
    let verdict = verify_onchain_use(&reader, &pinned, &proof, &OnchainUse { consumer, submission_id: submission, tx_hash });
    assert_eq!(verdict.outcome.name(), v["expected"].as_str().unwrap(), "{verdict:?}");
}

// -------------------------------------------------------------------------------------------
// Job K: the no-indexer path (Act transaction + consumer state)
// -------------------------------------------------------------------------------------------

use crate::{ActTransaction, ConsumerStateView, decode_consumer_state};

/// A reader with a stubbed Act transaction and consumer state, used for the no-indexer path.
struct TxReader {
    inner: JsonRpcReader<FakeRpc>,
    act: Option<ActTransaction>,
    consumer_state: Option<Vec<u8>>,
}

impl ChainReader for TxReader {
    fn actor_state(&self, account: &Bytes32, program: &Bytes32) -> Result<Vec<u8>, String> {
        if program == &[0xCC; 32] {
            return Ok(self.consumer_state.clone().unwrap_or_default());
        }
        self.inner.actor_state(account, program)
    }
    fn latest_timestamp(&self) -> Result<u64, String> {
        self.inner.latest_timestamp()
    }
    fn gated_events(&self, tx_hash: &Bytes32, consumer: &Bytes32) -> Result<Vec<EventRecord>, String> {
        self.inner.gated_events(tx_hash, consumer)
    }
    fn act_transaction(&self, _: &Bytes32) -> Result<Option<ActTransaction>, String> {
        Ok(self.act)
    }
}

fn consumer_state_bytes(nonce: u64) -> Vec<u8> {
    let mut b = nonce.to_le_bytes().to_vec();
    b.push(0); // pending: None
    b
}

fn act_tx(applied: bool, consumer: Bytes32, slot: [u8; 32]) -> ActTransaction {
    ActTransaction {
        actor: [0xA1; 32],
        consumer,
        slot,
        applied,
        block_id: Some(42),
    }
}

fn no_indexer_world(nonce: u64) -> (JsonRpcReader<FakeRpc>, ProofObject) {
    onchain_case_world(nonce)
}

fn onchain_case_world(_nonce: u64) -> (JsonRpcReader<FakeRpc>, ProofObject) {
    let (pinned, consumer, submission, tx_hash, actor, slot) = onchain_case();
    let context = onchain_context(&pinned, &consumer, &submission, &actor, 0);
    let event = gated_event(consumer, submission, slot, actor, 0);
    world_events(Some(good_onchain_fact(context)), Some(NB + 1), vec![event])
}

#[test]
fn no_indexer_an_act_transaction_and_its_state_bump_verify() {
    let (pinned, consumer, submission, tx_hash, _actor, slot) = onchain_case();
    let (inner, proof) = no_indexer_world(0);
    let reader = TxReader {
        inner,
        act: Some(act_tx(true, consumer, slot)),
        consumer_state: Some(consumer_state_bytes(1)),
    };
    let verdict = verify_onchain_tx(&reader, &pinned, &proof, &OnchainUse { consumer, submission_id: submission, tx_hash });
    assert_eq!(verdict.outcome, Outcome::Ok, "{verdict:?}");
}

#[test]
fn no_indexer_a_later_second_act_keeps_the_first_transaction_applied() {
    let (pinned, consumer, submission, tx_hash, _actor, slot) = onchain_case();
    let (inner, proof) = no_indexer_world(0);
    let reader = TxReader {
        inner,
        act: Some(act_tx(true, consumer, slot)),
        consumer_state: Some(consumer_state_bytes(5)), // A has acted several times since
    };
    let verdict = verify_onchain_tx(&reader, &pinned, &proof, &OnchainUse { consumer, submission_id: submission, tx_hash });
    // The slot fact is still the one for nonce 0 in this stub: the Act at 0 is verified as
    // applied and A's nonce moved beyond it. A newer prove would replace the fact (freshness).
    assert_eq!(verdict.outcome, Outcome::Ok, "{verdict:?}");
}

#[test]
fn no_indexer_a_reverted_transaction_is_refused() {
    let (pinned, consumer, submission, tx_hash, _actor, slot) = onchain_case();
    let (inner, proof) = no_indexer_world(0);
    let reader = TxReader {
        inner,
        act: Some(act_tx(false, consumer, slot)),
        consumer_state: Some(consumer_state_bytes(1)),
    };
    let verdict = verify_onchain_tx(&reader, &pinned, &proof, &OnchainUse { consumer, submission_id: submission, tx_hash });
    assert_eq!(verdict.outcome, Outcome::NoFact, "{verdict:?}");
}

#[test]
fn no_indexer_another_consumers_act_is_refused() {
    let (pinned, consumer, submission, tx_hash, _actor, slot) = onchain_case();
    let (inner, proof) = no_indexer_world(0);
    let reader = TxReader {
        inner,
        act: Some(act_tx(true, [0x99; 32], slot)),
        consumer_state: Some(consumer_state_bytes(1)),
    };
    let verdict = verify_onchain_tx(&reader, &pinned, &proof, &OnchainUse { consumer, submission_id: submission, tx_hash });
    assert_eq!(verdict.outcome, Outcome::BindingMismatch, "{verdict:?}");
}

#[test]
fn a_gated_event_that_disagrees_with_the_act_transaction_is_refused() {
    let (pinned, consumer, submission, tx_hash, actor, slot) = onchain_case();
    let context = onchain_context(&pinned, &consumer, &submission, &actor, 0);
    let event = gated_event(consumer, submission, slot, actor, 0);
    let (inner, proof) = world_events(Some(good_onchain_fact(context)), Some(NB + 1), vec![event]);
    let reader = TxReader {
        inner,
        act: Some(act_tx(true, consumer, [0x99; 32])), // a different slot than the event
        consumer_state: Some(consumer_state_bytes(10)),
    };
    let verdict = verify_onchain_use(&reader, &pinned, &proof, &OnchainUse { consumer, submission_id: submission, tx_hash });
    assert_eq!(verdict.outcome, Outcome::BindingMismatch, "{verdict:?}");
}

#[test]
fn a_consumer_state_decodes_and_rejects_garbage() {
    let decoded = decode_consumer_state(&consumer_state_bytes(7)).expect("state decodes");
    assert_eq!(decoded, ConsumerStateView { nonce: 7 });
    assert!(decode_consumer_state(&[1, 2, 3]).is_none());
}

#[test]
fn a_shared_act_transaction_vector_agrees_with_javascript() {
    let v: Value = serde_json::from_str(include_str!("../../../vectors/act-tx-vector.json")).unwrap();
    let unhexv = |k: &str| -> Bytes32 { unhex32(v[k].as_str().unwrap()).unwrap() };
    let pinned = Pinned {
        gate: unhexv("gate"),
        nft_program: unhexv("nft_program"),
        collection: unhexv("collection"),
        zone_id: unhexv("zone_id"),
    };
    let consumer = unhexv("consumer");
    let submission = unhexv("submission_id");
    let tx_hash = unhexv("tx_hash");
    let a = &v["act"];
    let act = ActTransaction {
        actor: unhex32(a["actor"].as_str().unwrap()).unwrap(),
        consumer: unhex32(a["consumer"].as_str().unwrap()).unwrap(),
        slot: unhex32(a["slot"].as_str().unwrap()).unwrap(),
        applied: a["applied"].as_bool().unwrap(),
        block_id: Some(42),
    };
    let fact: Vec<u8> = v["fact"].as_array().unwrap().iter().map(|b| b.as_u64().unwrap() as u8).collect();
    let (inner, proof) = world_events(
        Some(fact),
        Some(v["now_ms"].as_u64().unwrap()),
        vec![], // no indexer
    );
    let reader = TxReader {
        inner,
        act: Some(act),
        consumer_state: Some(consumer_state_bytes(v["consumer_nonce"].as_u64().unwrap())),
    };
    let verdict = verify_onchain_tx(
        &reader, &pinned, &proof,
        &OnchainUse { consumer, submission_id: submission, tx_hash },
    );
    assert_eq!(verdict.outcome.name(), v["expected"].as_str().unwrap(), "{verdict:?}");
}
