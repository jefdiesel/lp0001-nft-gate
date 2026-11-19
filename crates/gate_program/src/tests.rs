// SPDX-License-Identifier: MIT OR Apache-2.0
//! Handler tests in the NFT program's style: one hand-built `ReceiveInput` per case, compared
//! against the exact expected `Response`. Conversations are in `tests/conversation.rs`.

use super::*;
use borsh::{BorshDeserialize, BorshSerialize};
use lee_core::account::ActorState;
use nft_core::{NftCopy, NftMaster};

const GATE: AccountId = AccountId::new([0x6A; 32]);
const OTHER_PROGRAM: AccountId = AccountId::new([0xBB; 32]);
const HOLDER: AccountId = AccountId::new([4; 32]);
const OTHER_ACCOUNT: AccountId = AccountId::new([5; 32]);
const COLLECTION: AccountId = AccountId::new([1; 32]);
const OTHER_COLLECTION: AccountId = AccountId::new([2; 32]);
const SECRET: [u8; 32] = [0x5E; 32];
const CONTEXT: [u8; 32] = [0xC7; 32];
const NB: u64 = 1_763_500_200_000;

fn gate(account: AccountId) -> Actor {
    Actor::new(account, GATE)
}

fn input(receiver: Actor, from: Option<Actor>, is_authorized: bool, pre: &[u8], message: Vec<u8>) -> ReceiveInput {
    ReceiveInput { receiver, from, is_authorized, pre_state: ActorState::from(pre.to_vec()), message }
}

fn msg(m: &impl BorshSerialize) -> Vec<u8> {
    borsh::to_vec(m).unwrap()
}

fn prove(secret: Option<[u8; 32]>, not_before: u64, ttl_class: u8) -> Vec<u8> {
    msg(&GateMessage::Prove { collection: COLLECTION, context: CONTEXT, not_before, ttl_class, slot_secret: secret })
}

fn expected_binding() -> [u8; 32] {
    gate_core::binding::<RiscZeroSha>(NFT_PROGRAM.value(), COLLECTION.value(), &CONTEXT)
}

fn pending_holder(ttl_class: u8) -> Vec<u8> {
    encode(&GateState::Holder {
        secret: SECRET,
        pending: Some(Pending {
            collection: *COLLECTION.value(),
            binding: expected_binding(),
            not_before: NB,
            expires_at: NB + gate_core::TTL_MS[usize::from(ttl_class)],
        }),
    })
}

fn idle_holder() -> Vec<u8> {
    encode(&GateState::Holder { secret: SECRET, pending: None })
}

fn reply_of(state: Option<&NftState>) -> Vec<u8> {
    msg(&StateReply { state: ActorState::from(state.map(NftState::encode).unwrap_or_default()) })
}

fn nft_reply_input(state: Option<&NftState>, authorized: bool) -> ReceiveInput {
    input(gate(HOLDER), Some(Actor::new(HOLDER, NFT_PROGRAM)), authorized, &pending_holder(0), reply_of(state))
}

fn copy(definition: AccountId, owned: bool) -> NftState {
    NftState::Copy(NftCopy { definition_id: definition, owned })
}

fn master(definition: AccountId, print_balance: u128) -> NftState {
    NftState::Master(NftMaster { definition_id: definition, print_balance })
}

fn the_slot() -> [u8; 32] {
    gate_core::slot::<RiscZeroSha>(&SECRET)
}

fn the_pda() -> AccountId {
    slot_account(&GATE, &the_slot())
}

fn the_fact(ttl_class: u8) -> Fact {
    Fact { binding: expected_binding(), not_before: NB, expires_at: NB + gate_core::TTL_MS[usize::from(ttl_class)] }
}

// ----- 1. root -----

#[test]
fn a_first_proof_keeps_the_secret_reads_the_own_nft_actor_and_bounds_the_window() {
    let response = handle_message(&input(gate(HOLDER), None, true, &[], prove(Some(SECRET), NB, 0)));
    let expected = Response::set_state(pending_holder(0))
        .send_sync(Actor::new(HOLDER, NFT_PROGRAM), &NftMessage::ReadState)
        .try_timestamp_window(NB..NB + 3_600_000)
        .unwrap();
    assert_eq!(response, expected);
}

#[test]
fn a_later_proof_reuses_the_stored_secret() {
    let response = handle_message(&input(gate(HOLDER), None, true, &idle_holder(), prove(None, NB, 2)));
    let expected = Response::set_state(pending_holder(2))
        .send_sync(Actor::new(HOLDER, NFT_PROGRAM), &NftMessage::ReadState)
        .try_timestamp_window(NB..NB + 86_400_000)
        .unwrap();
    assert_eq!(response, expected);
}

#[test]
#[should_panic(expected = "gate: root not authorized")]
fn an_unauthorized_root_is_refused() {
    // T3's rule: an nsk-only witness gives is_authorized == false at the root.
    let _ = handle_message(&input(gate(HOLDER), None, false, &[], prove(Some(SECRET), NB, 0)));
}

#[test]
#[should_panic(expected = "gate: a first proof needs a slot secret")]
fn a_first_proof_without_a_secret_is_refused() {
    let _ = handle_message(&input(gate(HOLDER), None, true, &[], prove(None, NB, 0)));
}

#[test]
#[should_panic(expected = "gate: the slot secret is already set")]
fn a_second_secret_is_refused() {
    let _ = handle_message(&input(gate(HOLDER), None, true, &idle_holder(), prove(Some([1; 32]), NB, 0)));
}

#[test]
#[should_panic(expected = "gate: a proof is already pending")]
fn a_pending_proof_blocks_another() {
    let _ = handle_message(&input(gate(HOLDER), None, true, &pending_holder(0), prove(None, NB, 0)));
}

#[test]
#[should_panic(expected = "gate: a slot cannot prove")]
fn a_slot_cannot_prove() {
    let fact = encode(&GateState::Fact(the_fact(0)));
    let _ = handle_message(&input(gate(the_pda()), None, true, &fact, prove(None, NB, 0)));
}

#[test]
#[should_panic(expected = "gate: not_before is off the grid")]
fn an_off_grid_not_before_is_refused() {
    let _ = handle_message(&input(gate(HOLDER), None, true, &[], prove(Some(SECRET), NB + 1, 0)));
}

#[test]
#[should_panic(expected = "gate: bad ttl_class")]
fn a_bad_ttl_class_is_refused() {
    let _ = handle_message(&input(gate(HOLDER), None, true, &[], prove(Some(SECRET), NB, 3)));
}

#[test]
#[should_panic(expected = "gate: expiry overflows")]
fn an_overflowing_expiry_is_refused() {
    let top = u64::MAX - u64::MAX % gate_core::QUANT_MS;
    let _ = handle_message(&input(gate(HOLDER), None, true, &[], prove(Some(SECRET), top, 0)));
}

#[test]
#[should_panic(expected = "gate: ReadState is refused as a transaction root")]
fn read_state_is_refused_as_a_root() {
    let _ = handle_message(&input(gate(HOLDER), None, true, &[], msg(&GateMessage::ReadState)));
}

#[test]
#[should_panic(expected = "gate: TestProve exists only in the unsafe test gate")]
fn the_real_gate_refuses_test_prove() {
    let m = GateMessage::TestProve {
        nft_program: OTHER_PROGRAM,
        collection: COLLECTION,
        context: CONTEXT,
        not_before: NB,
        ttl_class: 0,
        slot_secret: Some(SECRET),
    };
    let _ = handle_message(&input(gate(HOLDER), None, true, &[], msg(&m)));
}

#[test]
#[should_panic(expected = "gate: message must decode")]
fn garbage_at_the_root_is_refused() {
    let _ = handle_message(&input(gate(HOLDER), None, true, &[], vec![9, 9, 9]));
}

// ----- 3. the NFT reply -----

#[test]
fn an_owned_copy_publishes_the_fact_to_the_slot() {
    let response = handle_message(&nft_reply_input(Some(&copy(COLLECTION, true)), true));
    let expected = Response::set_state(idle_holder()).send_sync(
        Actor::new(the_pda(), GATE),
        &GateInternal::Publish { slot: the_slot(), fact: FactFields::from(the_fact(0)) },
    );
    assert_eq!(response, expected);
}

#[test]
fn a_master_counts_as_holding() {
    // OWNER DECISION Q2, default: yes.
    assert!(gate_core::MASTER_COUNTS);
    let response = handle_message(&nft_reply_input(Some(&master(COLLECTION, 1)), true));
    assert_eq!(response.post_state, Some(ActorState::from(idle_holder())));
    assert_eq!(response.sync_messages.len(), 1);
}

#[test]
#[should_panic(expected = "gate: reply not authorized")]
fn an_unauthorized_reply_is_refused() {
    let _ = handle_message(&nft_reply_input(Some(&copy(COLLECTION, true)), false));
}

fn not_a_member(state: Option<&NftState>) {
    let r = std::panic::catch_unwind(|| handle_message(&nft_reply_input(state, true)));
    let err = r.expect_err("must refuse");
    let text = err
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| err.downcast_ref::<&str>().map(|s| (*s).to_owned()))
        .unwrap();
    assert!(text.contains("gate: NOT_A_MEMBER"), "{text}");
}

#[test]
fn every_non_holding_is_not_a_member() {
    not_a_member(Some(&copy(COLLECTION, false)));
    not_a_member(Some(&copy(OTHER_COLLECTION, true)));
    not_a_member(Some(&master(COLLECTION, 0)));
    not_a_member(Some(&master(OTHER_COLLECTION, 5)));
    not_a_member(Some(&NftState::Definition(nft_core::NftDefinition {
        name: "C".to_owned(),
        printable_supply: 3,
        metadata: nft_core::NftMetadata {
            standard: nft_core::MetadataStandard::Simple,
            uri: String::new(),
            creators: String::new(),
        },
    })));
    not_a_member(None);
}

#[test]
fn undecodable_nft_state_is_not_a_member_not_a_decode_panic() {
    let message = msg(&StateReply { state: ActorState::from(vec![2, 1, 2, 3]) });
    let r = std::panic::catch_unwind(|| {
        handle_message(&input(gate(HOLDER), Some(Actor::new(HOLDER, NFT_PROGRAM)), true, &pending_holder(0), message))
    });
    let err = r.expect_err("must refuse");
    let text = err
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| err.downcast_ref::<&str>().map(|s| (*s).to_owned()))
        .unwrap_or_default();
    assert!(text.contains("gate: NOT_A_MEMBER"), "{text}");
}

#[test]
#[should_panic(expected = "gate: message must decode")]
fn a_look_alike_nft_program_reply_is_not_taken_as_the_reply() {
    // (holder, OTHER_PROGRAM) answering "owned": the reply is not from (own account,
    // NFT_PROGRAM), so it falls to case 4, where it is no gate message.
    let _ = handle_message(&input(
        gate(HOLDER),
        Some(Actor::new(HOLDER, OTHER_PROGRAM)),
        true,
        &pending_holder(0),
        reply_of(Some(&copy(COLLECTION, true))),
    ));
}

#[test]
#[should_panic(expected = "gate: message must decode")]
fn the_nft_program_on_another_account_is_not_the_reply() {
    let _ = handle_message(&input(
        gate(HOLDER),
        Some(Actor::new(OTHER_ACCOUNT, NFT_PROGRAM)),
        true,
        &pending_holder(0),
        reply_of(Some(&copy(COLLECTION, true))),
    ));
}

// ----- 2. Publish -----

fn publish_input(receiver: AccountId, pre: &[u8], fact: Fact) -> ReceiveInput {
    input(
        gate(receiver),
        Some(Actor::new(AccountId::new([0x77; 32]), GATE)), // a blinded alias of the holder's gate actor
        false,
        pre,
        msg(&GateInternal::Publish { slot: the_slot(), fact: FactFields::from(fact) }),
    )
}

#[test]
fn publish_writes_the_fact_at_the_slot() {
    let response = handle_message(&publish_input(the_pda(), &[], the_fact(0)));
    assert_eq!(response, Response::set_state(encode(&GateState::Fact(the_fact(0)))));
    assert_eq!(response.post_state.as_ref().map(|s| s.len()), Some(gate_core::FACT_LEN));
}

#[test]
fn publish_overwrites_an_older_fact() {
    let old = Fact { binding: [9; 32], ..the_fact(1) };
    let response = handle_message(&publish_input(the_pda(), &encode(&GateState::Fact(old)), the_fact(0)));
    assert_eq!(response, Response::set_state(encode(&GateState::Fact(the_fact(0)))));
}

#[test]
#[should_panic(expected = "gate: Publish reached an actor that is not its slot")]
fn publish_elsewhere_is_refused() {
    let _ = handle_message(&publish_input(OTHER_ACCOUNT, &[], the_fact(0)));
}

#[test]
#[should_panic(expected = "gate: Publish reached a holder")]
fn publish_onto_a_holder_is_refused() {
    let _ = handle_message(&publish_input(the_pda(), &idle_holder(), the_fact(0)));
}

// ----- 4. other programs -----

#[test]
fn read_state_of_a_fact_replies_to_the_reader() {
    let fact = encode(&GateState::Fact(the_fact(0)));
    let reader = Actor::new(OTHER_ACCOUNT, OTHER_PROGRAM);
    let i = input(gate(the_pda()), Some(reader), true, &fact, msg(&GateMessage::ReadState));
    let response = handle_message(&i);
    assert_eq!(response, Response::keep_state().send_sync(reader, &StateReply::from(&i)));
}

#[test]
fn read_state_of_an_empty_slot_replies_empty() {
    let reader = Actor::new(OTHER_ACCOUNT, OTHER_PROGRAM);
    let i = input(gate(the_pda()), Some(reader), false, &[], msg(&GateMessage::ReadState));
    let response = handle_message(&i);
    assert_eq!(response, Response::keep_state().send_sync(reader, &StateReply { state: ActorState::empty() }));
}

#[test]
#[should_panic(expected = "gate: a holder's state is not readable")]
fn read_state_of_a_holder_is_refused() {
    let reader = Actor::new(OTHER_ACCOUNT, OTHER_PROGRAM);
    let _ = handle_message(&input(gate(HOLDER), Some(reader), true, &idle_holder(), msg(&GateMessage::ReadState)));
}

#[test]
#[should_panic(expected = "gate: Prove is accepted only as a transaction root")]
fn prove_from_another_program_is_refused() {
    let reader = Actor::new(OTHER_ACCOUNT, OTHER_PROGRAM);
    let _ = handle_message(&input(gate(HOLDER), Some(reader), true, &[], prove(Some(SECRET), NB, 0)));
}

// ----- layout and derivations -----

/// The spec's state enum written as plain borsh derives: gate_core's hand encoding must
/// produce the same bytes.
#[derive(BorshSerialize, BorshDeserialize, PartialEq, Debug)]
enum BorshGateState {
    Holder { version: u8, secret: [u8; 32], pending: Option<BorshPending> },
    Fact { version: u8, binding: [u8; 32], not_before: u64, expires_at: u64 },
}

#[derive(BorshSerialize, BorshDeserialize, PartialEq, Debug)]
struct BorshPending {
    collection: AccountId,
    binding: [u8; 32],
    not_before: u64,
    expires_at: u64,
}

#[test]
fn hand_encoding_is_borsh() {
    let f = the_fact(1);
    let b = BorshGateState::Fact { version: 1, binding: f.binding, not_before: f.not_before, expires_at: f.expires_at };
    assert_eq!(borsh::to_vec(&b).unwrap(), encode(&GateState::Fact(f)));
    assert_eq!(borsh::to_vec(&b).unwrap().len(), 50);
    let idle = BorshGateState::Holder { version: 1, secret: SECRET, pending: None };
    assert_eq!(borsh::to_vec(&idle).unwrap(), idle_holder());
    let pending = BorshGateState::Holder {
        version: 1,
        secret: SECRET,
        pending: Some(BorshPending {
            collection: COLLECTION,
            binding: expected_binding(),
            not_before: NB,
            expires_at: NB + 3_600_000,
        }),
    };
    assert_eq!(borsh::to_vec(&pending).unwrap(), pending_holder(0));
}

#[test]
fn gate_core_pda_is_lee_pda() {
    for seed in [[0_u8; 32], [0xFF; 32], SECRET, the_slot()] {
        let lee = AccountId::for_public_pda(&GATE, &PdaSeed::new(seed));
        assert_eq!(*lee.value(), gate_core::slot_pda::<gate_core::Sha2>(GATE.value(), &seed));
    }
}

#[test]
fn risc0_sha_is_sha256() {
    let parts: [&[u8]; 3] = [b"lp0001", b"", &[1, 2, 3]];
    assert_eq!(
        <RiscZeroSha as Sha256>::hash(&parts),
        <gate_core::Sha2 as Sha256>::hash(&parts)
    );
}

#[test]
fn nft_read_state_message_is_the_reference_encoding() {
    // The gate sends the reference program's own `Message::ReadState` (tag 5).
    assert_eq!(borsh::to_vec(&NftMessage::ReadState).unwrap(), vec![5]);
}

#[test]
fn the_test_nft_program_constant_is_the_default() {
    // Guards against a native test silently running with a deploy value.
    if option_env!("LP0001_NFT_PROGRAM").is_none() {
        assert_eq!(NFT_PROGRAM, AccountId::new([0xAA; 32]));
    }
}
