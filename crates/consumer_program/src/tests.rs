// SPDX-License-Identifier: MIT OR Apache-2.0
//! Consumer handler tests, NFT-program style, and the T10 agreement test against the shared
//! vector file.

use super::*;
use borsh::BorshSerialize;
use lee_core::account::ActorState;

const CONSUMER: AccountId = AccountId::new([0xCC; 32]);
const A: AccountId = AccountId::new([0xA1; 32]);
const SLOT: [u8; 32] = [0x51; 32];
const NB: u64 = 1_763_500_200_000;

fn me(account: AccountId) -> Actor {
    Actor::new(account, CONSUMER)
}

fn input(receiver: Actor, from: Option<Actor>, is_authorized: bool, pre: &ConsumerState, message: Vec<u8>) -> ReceiveInput {
    let pre_bytes = if *pre == ConsumerState::default() { Vec::new() } else { encode(pre) };
    ReceiveInput { receiver, from, is_authorized, pre_state: ActorState::from(pre_bytes), message }
}

fn msg(m: &impl BorshSerialize) -> Vec<u8> {
    borsh::to_vec(m).unwrap()
}

fn reply(state: &[u8]) -> Vec<u8> {
    msg(&StateReply { state: ActorState::from(state.to_vec()) })
}

fn pending(nonce: u64) -> ConsumerState {
    ConsumerState { nonce, pending: Some(SLOT) }
}

fn fact_for(nonce: u64, ttl: u64) -> Vec<u8> {
    gate_core::Fact { binding: expected_binding(&CONSUMER, &A, nonce), not_before: NB, expires_at: NB + ttl }
        .encode()
        .to_vec()
}

fn panic_text(f: impl FnOnce() -> Response + std::panic::UnwindSafe) -> String {
    let err = std::panic::catch_unwind(f).expect_err("must panic");
    err.downcast_ref::<String>()
        .cloned()
        .or_else(|| err.downcast_ref::<&str>().map(|s| (*s).to_owned()))
        .unwrap_or_default()
}

#[test]
fn act_reads_the_slot_from_the_pinned_gate() {
    let response = handle_message(&input(me(A), None, true, &ConsumerState::default(), msg(&ConsumerMessage::Act { slot: SLOT })));
    let expected = Response::set_state(encode(&pending(0))).send_sync(slot_actor(&SLOT), &GateMessage::ReadState);
    assert_eq!(response, expected);
    assert_eq!(slot_actor(&SLOT).program_account_id, GATE);
}

#[test]
fn act_unsigned_is_not_authorized() {
    let t = panic_text(|| handle_message(&input(me(A), None, false, &ConsumerState::default(), msg(&ConsumerMessage::Act { slot: SLOT }))));
    assert!(t.contains("consumer: NOT_AUTHORIZED"), "{t}");
}

#[test]
fn an_ok_fact_emits_gated_bumps_the_nonce_and_bounds_the_window() {
    let response = handle_message(&input(me(A), Some(slot_actor(&SLOT)), true, &pending(0), reply(&fact_for(0, 3_600_000))));
    let event = Gated { submission_id: SUBMISSION_ID, slot: SLOT, actor: A, nonce: 0, expires_at: NB + 3_600_000 };
    let expected = Response::set_state(encode(&ConsumerState { nonce: 1, pending: None }))
        .event(ProgramEvent { selector: gated_selector(), data: borsh::to_vec(&event).unwrap() })
        .try_timestamp_window(NB..NB + 3_600_000)
        .unwrap();
    assert_eq!(response, expected);
}

#[test]
fn the_same_fact_after_the_bump_is_binding_mismatch() {
    let t = panic_text(|| handle_message(&input(me(A), Some(slot_actor(&SLOT)), true, &pending(1), reply(&fact_for(0, 3_600_000)))));
    assert!(t.contains("consumer: BINDING_MISMATCH"), "{t}");
}

#[test]
fn another_account_using_the_fact_is_binding_mismatch() {
    let b = AccountId::new([0xB2; 32]);
    let t = panic_text(|| {
        handle_message(&input(Actor::new(b, CONSUMER), Some(slot_actor(&SLOT)), true, &pending(0), reply(&fact_for(0, 3_600_000))))
    });
    assert!(t.contains("consumer: BINDING_MISMATCH"), "{t}");
}

#[test]
fn an_empty_slot_is_no_fact() {
    let t = panic_text(|| handle_message(&input(me(A), Some(slot_actor(&SLOT)), true, &pending(0), reply(&[]))));
    assert!(t.contains("consumer: NO_FACT"), "{t}");
}

#[test]
fn a_reply_from_anyone_but_the_slot_is_refused() {
    let fake = Actor::new(AccountId::new([0x99; 32]), GATE);
    let t = panic_text(|| handle_message(&input(me(A), Some(fake), true, &pending(0), reply(&fact_for(0, 3_600_000)))));
    assert!(t.contains("not from the slot's gate actor"), "{t}");
    let other_program = Actor::new(slot_actor(&SLOT).account_id, AccountId::new([0x98; 32]));
    let t = panic_text(|| handle_message(&input(me(A), Some(other_program), true, &pending(0), reply(&fact_for(0, 3_600_000)))));
    assert!(t.contains("not from the slot's gate actor"), "{t}");
}

#[test]
fn a_message_with_nothing_pending_is_refused() {
    let t = panic_text(|| handle_message(&input(me(A), Some(slot_actor(&SLOT)), true, &ConsumerState::default(), reply(&[]))));
    assert!(t.contains("consumer: unexpected message"), "{t}");
}

#[test]
fn a_second_act_while_pending_is_refused() {
    let t = panic_text(|| handle_message(&input(me(A), None, true, &pending(0), msg(&ConsumerMessage::Act { slot: SLOT }))));
    assert!(t.contains("an Act is already pending"), "{t}");
}

// ----- T10: the consumer and check() agree on the shared vector file -----

fn hx(v: &serde_json::Value) -> Vec<u8> {
    let s = v.as_str().unwrap();
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

#[test]
fn t10_consumer_agrees_with_check_on_every_vector() {
    let v: serde_json::Value = serde_json::from_str(include_str!("../../../vectors/gate-vectors.json")).unwrap();
    let cases = v["consumer_cases"].as_array().unwrap();
    assert!(cases.len() >= 10);
    // The vector file is built for these constants and this consumer and actor.
    assert_eq!(hx(&cases[0]["expected_binding"]), expected_binding(&CONSUMER, &A, 0).to_vec());
    for c in cases {
        let name = c["name"].as_str().unwrap();
        let state = hx(&c["state"]);
        let want = c["onchain_outcome"].as_str().unwrap();
        let lib = gate_core::check(&state, &expected_binding(&CONSUMER, &A, 0), None).name();
        assert_eq!(lib, want, "{name}: library");
        let s = state.clone();
        let consumer = std::panic::catch_unwind(move || {
            handle_message(&input(me(A), Some(slot_actor(&SLOT)), true, &pending(0), reply(&s)))
        });
        let got = match consumer {
            Ok(_) => "OK".to_owned(),
            Err(e) => {
                let t = e.downcast_ref::<String>().cloned().unwrap_or_default();
                t.strip_prefix("consumer: ").unwrap_or(&t).to_owned()
            }
        };
        assert_eq!(got, want, "{name}: consumer");
    }
}
