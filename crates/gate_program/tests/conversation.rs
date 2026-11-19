// SPDX-License-Identifier: MIT OR Apache-2.0
//! Whole conversations, natively: the gate, the reference NFT program's own handler
//! (`nft_program` at lez-programs dea23c2) and the reference consumer, run by a small
//! stand-in for the LEZ executor.
//!
//! The stand-in is NOT LEZ. It delivers sync messages depth-first, as LEZ's work stack does,
//! gives every actor on a signing account `is_authorized`, intersects validity windows and
//! refuses a transaction whose window misses the chain time, and rolls back on any panic. It
//! does not model private execution, blinded aliases, admission, fees or proofs: those are
//! what the executor tests in RUN-AFTER-3AM.txt (T1-T13) are for.

use std::collections::BTreeMap;
use std::panic::{catch_unwind, AssertUnwindSafe};

use gate_core::{Outcome, Sha2};
use gate_interface::{ConsumerMessage, ConsumerState, Gated, GateMessage};
use lee_core::{
    account::{AccountId, Actor, ActorState},
    program::{ProgramEvent, ReceiveInput, Response, ValidityWindows},
};
use nft_core::{Message as NftMessage, NftCopy, NftMaster, NftState};

const GATE: AccountId = AccountId::new([0x6A; 32]); // = consumer_program::GATE (test default)
const NFT: AccountId = AccountId::new([0xAA; 32]); // = gate_program::NFT_PROGRAM (test default)
const CONSUMER: AccountId = AccountId::new([0xCC; 32]);
const COLLECTION: AccountId = AccountId::new([0xC0; 32]); // = consumer_program::COLLECTION
const OTHER_COLLECTION: AccountId = AccountId::new([0xC1; 32]);
const H1: AccountId = AccountId::new([0x11; 32]);
const H2: AccountId = AccountId::new([0x12; 32]);
const NOBODY: AccountId = AccountId::new([0x13; 32]);
const MASTER_HOLDER: AccountId = AccountId::new([0x14; 32]);
const A: AccountId = AccountId::new([0xA1; 32]);
const B: AccountId = AccountId::new([0xB2; 32]);
const NB: u64 = 1_763_500_200_000;
const HOUR: u64 = 3_600_000;

type Handler = fn(&ReceiveInput) -> Response;

struct Chain {
    programs: BTreeMap<AccountId, Handler>,
    states: BTreeMap<Actor, Vec<u8>>,
    /// The sequencer's clock for the next transaction.
    now: u64,
}

#[derive(Debug)]
struct Applied {
    events: Vec<(Actor, ProgramEvent)>,
    window: ValidityWindows,
}

fn panic_text(e: &(dyn std::any::Any + Send)) -> String {
    e.downcast_ref::<String>()
        .cloned()
        .or_else(|| e.downcast_ref::<&str>().map(|s| (*s).to_owned()))
        .unwrap_or_else(|| "panic".to_owned())
}

impl Chain {
    fn new() -> Self {
        std::panic::set_hook(Box::new(|_| {}));
        let mut programs: BTreeMap<AccountId, Handler> = BTreeMap::new();
        programs.insert(GATE, gate_program::handle_message);
        programs.insert(NFT, nft_program::handle_message);
        programs.insert(CONSUMER, consumer_program::handle_message);
        Self { programs, states: BTreeMap::new(), now: NB + 60_000 }
    }

    fn set(&mut self, actor: Actor, bytes: Vec<u8>) {
        self.states.insert(actor, bytes);
    }

    fn get(&self, actor: Actor) -> Vec<u8> {
        self.states.get(&actor).cloned().unwrap_or_default()
    }

    /// One transaction: `message` to `root`, signed by `signers`. All or nothing.
    fn send(&mut self, root: Actor, signers: &[AccountId], message: Vec<u8>) -> Result<Applied, String> {
        let snapshot = self.states.clone();
        let result = self.run(root, signers, message);
        if result.is_err() {
            self.states = snapshot;
        }
        result
    }

    fn run(&mut self, root: Actor, signers: &[AccountId], message: Vec<u8>) -> Result<Applied, String> {
        let mut window = ValidityWindows::new_unbounded();
        let mut events = Vec::new();
        let mut asyncs = Vec::new();
        let mut stack: Vec<(Actor, Option<Actor>, Vec<u8>, bool)> = vec![(root, None, message, true)];
        while let Some((to, from, message, may_authorize)) = stack.pop() {
            let handler = *self.programs.get(&to.program_account_id).ok_or("no such program")?;
            let input = ReceiveInput {
                receiver: to,
                from,
                is_authorized: may_authorize && signers.contains(&to.account_id),
                pre_state: ActorState::from(self.get(to)),
                message,
            };
            let response = catch_unwind(AssertUnwindSafe(|| handler(&input))).map_err(|e| panic_text(&*e))?;
            window = window
                .intersect(response.validity)
                .map_err(|_| "EmptyValidityWindowIntersection".to_owned())?;
            if let Some(post) = response.post_state {
                if post.is_empty() {
                    self.states.remove(&to);
                } else {
                    self.states.insert(to, post.to_vec());
                }
            }
            events.extend(response.events.into_iter().map(|e| (to, e)));
            for m in response.sync_messages.into_iter().rev() {
                stack.push((m.to, Some(to), m.message, true));
            }
            asyncs.extend(response.async_messages.into_iter().map(|m| (m.to, to, m.message)));
        }
        if !window.is_valid_at(1, self.now) {
            return Err("OutOfValidityWindow".to_owned());
        }
        // Async messages are received at once, with no authorisation (a promoted message).
        for (to, from, message) in asyncs {
            let mut stack = vec![(to, Some(from), message, false)];
            while let Some((to, from, message, _)) = stack.pop() {
                let handler = *self.programs.get(&to.program_account_id).ok_or("no such program")?;
                let input = ReceiveInput {
                    receiver: to,
                    from,
                    is_authorized: false,
                    pre_state: ActorState::from(self.get(to)),
                    message,
                };
                let response = catch_unwind(AssertUnwindSafe(|| handler(&input))).map_err(|e| panic_text(&*e))?;
                if let Some(post) = response.post_state {
                    self.states.insert(to, post.to_vec());
                }
                for m in response.sync_messages.into_iter().rev() {
                    stack.push((m.to, Some(to), m.message, false));
                }
            }
        }
        Ok(Applied { events, window })
    }
}

fn copy(owned: bool) -> Vec<u8> {
    NftState::Copy(NftCopy { definition_id: COLLECTION, owned }).encode()
}

fn prove_msg(context: [u8; 32], not_before: u64, ttl_class: u8, secret: Option<[u8; 32]>) -> Vec<u8> {
    borsh::to_vec(&GateMessage::Prove { collection: COLLECTION, context, not_before, ttl_class, slot_secret: secret }).unwrap()
}

fn offchain_context(nonce: u8) -> [u8; 32] {
    gate_core::context_offchain::<Sha2>(&consumer_program::ZONE_ID, GATE.value(), b"app.example", b"Check in", &[nonce; 32])
}

fn onchain_context(actor: AccountId, nonce: u64) -> [u8; 32] {
    gate_core::context_onchain::<Sha2>(
        &consumer_program::ZONE_ID,
        GATE.value(),
        CONSUMER.value(),
        &consumer_program::SUBMISSION_ID,
        actor.value(),
        nonce,
    )
}

fn binding(context: &[u8; 32]) -> [u8; 32] {
    gate_core::binding::<Sha2>(NFT.value(), COLLECTION.value(), context)
}

fn slot_pda(secret: &[u8; 32]) -> Actor {
    let slot = gate_core::slot::<Sha2>(secret);
    Actor::new(AccountId::new(gate_core::slot_pda::<Sha2>(GATE.value(), &slot)), GATE)
}

fn with_holder(chain: &mut Chain, holder: AccountId, nft_state: Vec<u8>) {
    chain.set(Actor::new(holder, NFT), nft_state);
}

#[test]
fn a_holder_proves_and_the_fact_checks_ok() {
    let mut c = Chain::new();
    with_holder(&mut c, H1, copy(true));
    let secret = [1; 32];
    let ctx = offchain_context(1);
    let before_nft = c.get(Actor::new(H1, NFT));
    let applied = c.send(Actor::new(H1, GATE), &[H1], prove_msg(ctx, NB, 0, Some(secret))).expect("applied");
    // One fact at the slot, OK for this context at a time inside the window.
    let fact = c.get(slot_pda(&secret));
    assert_eq!(fact.len(), 50);
    assert_eq!(gate_core::check(&fact, &binding(&ctx), Some(NB + 1)), Outcome::Ok);
    // The window is the fact's window: a non-empty window bounds the transaction.
    assert!(applied.window.is_valid_at(1, NB) && !applied.window.is_valid_at(1, NB + HOUR) && !applied.window.is_valid_at(1, NB - 1));
    // Outcome check, natively: the NFT actor is byte-identical; the holder's gate actor is an
    // idle Holder; nothing else but the slot changed.
    assert_eq!(c.get(Actor::new(H1, NFT)), before_nft);
    assert_eq!(
        gate_core::GateState::decode(&c.get(Actor::new(H1, GATE))),
        Ok(gate_core::GateState::Holder { secret, pending: None })
    );
    assert_eq!(c.states.len(), 3);
}

#[test]
fn a_non_holder_is_refused_and_nothing_is_written() {
    let mut c = Chain::new();
    with_holder(&mut c, NOBODY, copy(false));
    let secret = [2; 32];
    let err = c.send(Actor::new(NOBODY, GATE), &[NOBODY], prove_msg(offchain_context(1), NB, 0, Some(secret))).unwrap_err();
    assert!(err.contains("gate: NOT_A_MEMBER"), "{err}");
    assert!(c.get(slot_pda(&secret)).is_empty());
    assert!(c.get(Actor::new(NOBODY, GATE)).is_empty());
}

#[test]
fn an_account_without_any_nft_holding_is_refused() {
    let mut c = Chain::new();
    let err = c.send(Actor::new(NOBODY, GATE), &[NOBODY], prove_msg(offchain_context(1), NB, 0, Some([3; 32]))).unwrap_err();
    assert!(err.contains("gate: NOT_A_MEMBER"), "{err}");
}

#[test]
fn an_unsigned_root_is_refused() {
    let mut c = Chain::new();
    with_holder(&mut c, H1, copy(true));
    let err = c.send(Actor::new(H1, GATE), &[], prove_msg(offchain_context(1), NB, 0, Some([1; 32]))).unwrap_err();
    assert_eq!(err, "gate: root not authorized");
}

#[test]
fn a_master_holder_proves() {
    let mut c = Chain::new();
    with_holder(&mut c, MASTER_HOLDER, NftState::Master(NftMaster { definition_id: COLLECTION, print_balance: 1 }).encode());
    let secret = [4; 32];
    c.send(Actor::new(MASTER_HOLDER, GATE), &[MASTER_HOLDER], prove_msg(offchain_context(1), NB, 0, Some(secret))).expect("applied");
    assert_eq!(gate_core::check(&c.get(slot_pda(&secret)), &binding(&offchain_context(1)), Some(NB)), Outcome::Ok);
}

#[test]
fn a_copy_of_another_collection_is_refused() {
    let mut c = Chain::new();
    with_holder(&mut c, H1, NftState::Copy(NftCopy { definition_id: OTHER_COLLECTION, owned: true }).encode());
    let err = c.send(Actor::new(H1, GATE), &[H1], prove_msg(offchain_context(1), NB, 0, Some([1; 32]))).unwrap_err();
    assert!(err.contains("NOT_A_MEMBER"));
}

#[test]
fn a_second_proof_rewrites_the_same_slot_and_supersedes_the_first() {
    // T7, natively.
    let mut c = Chain::new();
    with_holder(&mut c, H1, copy(true));
    let secret = [1; 32];
    c.send(Actor::new(H1, GATE), &[H1], prove_msg(offchain_context(1), NB, 0, Some(secret))).unwrap();
    // The wallet sends no secret the second time; the gate keeps the stored one.
    c.send(Actor::new(H1, GATE), &[H1], prove_msg(offchain_context(2), NB, 1, None)).unwrap();
    let fact = c.get(slot_pda(&secret));
    assert_eq!(gate_core::check(&fact, &binding(&offchain_context(1)), Some(NB + 1)), Outcome::BindingMismatch);
    assert_eq!(gate_core::check(&fact, &binding(&offchain_context(2)), Some(NB + 1)), Outcome::Ok);
    // Exactly one gate fact exists.
    assert_eq!(c.states.keys().filter(|a| a.program_account_id == GATE && a.account_id != H1).count(), 1);
}

#[test]
fn the_sequencer_refuses_outside_the_window() {
    // T6, natively: the stand-in sequencer applies the transaction's window to its clock.
    let mut c = Chain::new();
    with_holder(&mut c, H1, copy(true));
    c.now = NB + HOUR; // at expires_at: excluded
    let err = c.send(Actor::new(H1, GATE), &[H1], prove_msg(offchain_context(1), NB, 0, Some([1; 32]))).unwrap_err();
    assert_eq!(err, "OutOfValidityWindow");
    c.now = NB - 1; // before not_before
    let err = c.send(Actor::new(H1, GATE), &[H1], prove_msg(offchain_context(1), NB, 0, Some([1; 32]))).unwrap_err();
    assert_eq!(err, "OutOfValidityWindow");
    c.now = NB;
    let err = c.send(Actor::new(H1, GATE), &[H1], prove_msg(offchain_context(1), NB + 1, 0, Some([1; 32]))).unwrap_err();
    assert_eq!(err, "gate: not_before is off the grid");
    let err = c.send(Actor::new(H1, GATE), &[H1], prove_msg(offchain_context(1), NB, 3, Some([1; 32]))).unwrap_err();
    assert_eq!(err, "gate: bad ttl_class");
    c.send(Actor::new(H1, GATE), &[H1], prove_msg(offchain_context(1), NB, 0, Some([1; 32]))).expect("inside");
}

fn act(c: &mut Chain, actor: AccountId, signed: bool, slot: [u8; 32]) -> Result<Applied, String> {
    let signers: Vec<AccountId> = if signed { vec![actor] } else { vec![] };
    c.send(Actor::new(actor, CONSUMER), &signers, borsh::to_vec(&ConsumerMessage::Act { slot }).unwrap())
}

#[test]
fn the_consumer_gates_once_per_fact_for_the_named_account() {
    // T8, natively.
    let mut c = Chain::new();
    with_holder(&mut c, H1, copy(true));
    let secret = [1; 32];
    let slot = gate_core::slot::<Sha2>(&secret);
    c.send(Actor::new(H1, GATE), &[H1], prove_msg(onchain_context(A, 0), NB, 0, Some(secret))).unwrap();

    // Another account pointing the consumer at A's fact.
    let err = act(&mut c, B, true, slot).unwrap_err();
    assert_eq!(err, "consumer: BINDING_MISMATCH");
    // A, unsigned.
    let err = act(&mut c, A, false, slot).unwrap_err();
    assert_eq!(err, "consumer: NOT_AUTHORIZED");
    // A, signed: once.
    let applied = act(&mut c, A, true, slot).expect("A acts");
    assert_eq!(applied.events.len(), 1);
    let (emitter, event) = &applied.events[0];
    assert_eq!(*emitter, Actor::new(A, CONSUMER));
    assert_eq!(event.selector, consumer_program::gated_selector());
    let gated: Gated = borsh::from_slice(&event.data).unwrap();
    assert_eq!(gated.nonce, 0);
    assert_eq!(gated.actor, A);
    assert_eq!(gated.submission_id, consumer_program::SUBMISSION_ID);
    assert_eq!(gated.expires_at, NB + HOUR);
    let state: ConsumerState = borsh::from_slice(&c.get(Actor::new(A, CONSUMER))).unwrap();
    assert_eq!(state, ConsumerState { nonce: 1, pending: None });
    // Again: the nonce moved, so the context did.
    let err = act(&mut c, A, true, slot).unwrap_err();
    assert_eq!(err, "consumer: BINDING_MISMATCH");
    // A slot nobody wrote.
    let err = act(&mut c, A, true, [0x77; 32]).unwrap_err();
    assert_eq!(err, "consumer: NO_FACT");
}

#[test]
fn the_sequencer_refuses_a_late_act() {
    let mut c = Chain::new();
    with_holder(&mut c, H1, copy(true));
    let secret = [1; 32];
    c.send(Actor::new(H1, GATE), &[H1], prove_msg(onchain_context(A, 0), NB, 0, Some(secret))).unwrap();
    c.now = NB + HOUR;
    let err = act(&mut c, A, true, gate_core::slot::<Sha2>(&secret)).unwrap_err();
    assert_eq!(err, "OutOfValidityWindow");
    // Nothing moved: A's nonce is still 0.
    assert!(c.get(Actor::new(A, CONSUMER)).is_empty());
}

#[test]
fn a_transfer_leaves_the_old_fact_live_and_the_new_holder_gets_a_new_slot() {
    // T9 / limit L2, natively, with the reference NFT program's own Transfer.
    let mut c = Chain::new();
    with_holder(&mut c, H1, copy(true));
    with_holder(&mut c, H2, copy(false));
    let (s1, s2) = ([1; 32], [2; 32]);
    let ctx = offchain_context(1);
    c.send(Actor::new(H1, GATE), &[H1], prove_msg(ctx, NB, 0, Some(s1))).unwrap();
    c.send(Actor::new(H1, NFT), &[H1], borsh::to_vec(&NftMessage::Transfer { to: H2 }).unwrap()).expect("transfer");
    assert_eq!(c.get(Actor::new(H1, NFT)), copy(false));
    assert_eq!(c.get(Actor::new(H2, NFT)), copy(true));
    // H1 can no longer prove ...
    let err = c.send(Actor::new(H1, GATE), &[H1], prove_msg(offchain_context(3), NB, 0, None)).unwrap_err();
    assert!(err.contains("NOT_A_MEMBER"));
    // ... but its fact stays live until it expires (L2).
    assert_eq!(gate_core::check(&c.get(slot_pda(&s1)), &binding(&ctx), Some(NB + 10)), Outcome::Ok);
    // H2 proves into its own, different slot.
    c.send(Actor::new(H2, GATE), &[H2], prove_msg(ctx, NB, 0, Some(s2))).unwrap();
    assert_ne!(slot_pda(&s1), slot_pda(&s2));
    assert_eq!(gate_core::check(&c.get(slot_pda(&s2)), &binding(&ctx), Some(NB + 10)), Outcome::Ok);
}

#[test]
fn the_gate_refuses_to_be_read_where_a_holder_lives() {
    let mut c = Chain::new();
    with_holder(&mut c, H1, copy(true));
    c.send(Actor::new(H1, GATE), &[H1], prove_msg(offchain_context(1), NB, 0, Some([1; 32]))).unwrap();
    // A consumer pointed at the holder's own gate actor instead of a slot cannot read the
    // secret: Act derives the PDA from the slot, so it never names H1; a direct ReadState from
    // another program is refused.
    c.programs.insert(B, |input: &ReceiveInput| {
        Response::keep_state().send_sync(Actor::new(AccountId::new([0x11; 32]), GATE), &GateMessage::ReadState)
            .event(ProgramEvent { selector: [0; 8], data: input.message.clone() })
    });
    let err = c.send(Actor::new(B, B), &[B], vec![]).unwrap_err();
    assert_eq!(err, "gate: a holder's state is not readable");
}

#[test]
fn an_identical_prove_a_second_time_is_refused_by_the_gate_for_a_second_slot_secret() {
    // Spec §2.8.1 is a chain-level refusal: the same private transaction's nullifier is
    // already seen in LEZ state. This native stand-in has no nullifier set, so it cannot
    // show point 1. It shows what the gate does today with the same Prove bytes twice:
    // the first proof stores the secret; the second carries it again and is refused by
    // §2.6.1's "the slot secret is already set" rule, not by a nullifier check.
    // The point-4 rewrite (a second proof with slot_secret None) has its own test above.
    let mut c = Chain::new();
    with_holder(&mut c, H1, copy(true));
    let secret = [1; 32];
    let message = prove_msg(offchain_context(1), NB, 0, Some(secret));
    c.send(Actor::new(H1, GATE), &[H1], message.clone()).expect("first proof");
    let fact_at = slot_pda(&secret);
    let fact = c.get(fact_at);
    assert_eq!(gate_core::check(&fact, &binding(&offchain_context(1)), Some(NB + 1)), Outcome::Ok);

    let err = c.send(Actor::new(H1, GATE), &[H1], message).unwrap_err();
    assert_eq!(err, "gate: the slot secret is already set");
    // Nothing changed: the fact, the idle holder and the NFT actor are byte-identical.
    assert_eq!(c.get(fact_at), fact);
    assert_eq!(
        gate_core::GateState::decode(&c.get(Actor::new(H1, GATE))),
        Ok(gate_core::GateState::Holder { secret, pending: None })
    );
    assert_eq!(c.get(Actor::new(H1, NFT)), copy(true));
}
