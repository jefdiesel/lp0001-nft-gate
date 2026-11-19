// SPDX-License-Identifier: MIT OR Apache-2.0
//! The LP-0001 reference consumer (spec §3.1): a LEZ program that gates one action on a gate
//! fact, once per fact, for the acting public account that signed.
//!
//! A program author who wants to gate copies the handler pair (`act` and `gate_reply`),
//! changes the deploy constants and replaces the gated action (here: an event and a nonce
//! bump).
//!
//! Deploy constants, never taken from the caller (that closes the look-alike NFT program
//! hole): `GATE`, `NFT_PROGRAM`, `COLLECTION`, `SUBMISSION_ID`, `ZONE_ID`. Set each as 64 hex
//! characters in the environment variable of the same name with the `LP0001_` prefix when
//! building the guest; native tests use fixed test values.

use borsh::BorshDeserialize as _;
use gate_core::{Bytes32, Outcome};
use gate_interface::{ConsumerMessage, ConsumerState, Gated, GateMessage, GATED_SELECTOR_NAME};
use lee_core::{
    account::{AccountId, Actor},
    program::{PdaSeed, ProgramEvent, ReceiveInput, Response, StateReply},
};

/// The gate this consumer trusts.
pub const GATE: AccountId = AccountId::new(gate_core::deploy_constant(option_env!("LP0001_GATE"), [0x6A; 32]));
/// The reference NFT program the gate is pinned to.
pub const NFT_PROGRAM: AccountId =
    AccountId::new(gate_core::deploy_constant(option_env!("LP0001_NFT_PROGRAM"), [0xAA; 32]));
/// The collection (NFT definition) this consumer gates on.
pub const COLLECTION: AccountId =
    AccountId::new(gate_core::deploy_constant(option_env!("LP0001_COLLECTION"), [0xC0; 32]));
/// The LP submission's unique identifier, carried by every gated action (prize line 45).
pub const SUBMISSION_ID: Bytes32 = gate_core::deploy_constant(option_env!("LP0001_SUBMISSION_ID"), [0x5B; 32]);
/// The zone's channel id (RPC `getChannelId`). A program cannot read it, so it is a constant.
pub const ZONE_ID: Bytes32 = gate_core::deploy_constant(option_env!("LP0001_ZONE_ID"), [0x2E; 32]);

#[cfg(target_os = "zkvm")]
const _: () = assert!(
    option_env!("LP0001_GATE").is_some()
        && option_env!("LP0001_NFT_PROGRAM").is_some()
        && option_env!("LP0001_COLLECTION").is_some()
        && option_env!("LP0001_SUBMISSION_ID").is_some()
        && option_env!("LP0001_ZONE_ID").is_some(),
    "set LP0001_GATE, LP0001_NFT_PROGRAM, LP0001_COLLECTION, LP0001_SUBMISSION_ID, LP0001_ZONE_ID"
);

/// SHA-256 through `risc0_zkvm::sha`.
pub struct RiscZeroSha;

impl gate_core::Sha256 for RiscZeroSha {
    fn hash(parts: &[&[u8]]) -> Bytes32 {
        use risc0_zkvm::sha::{Impl, Sha256 as _};
        let mut buf = Vec::with_capacity(parts.iter().map(|p| p.len()).sum());
        for part in parts {
            buf.extend_from_slice(part);
        }
        let mut out = [0_u8; 32];
        out.copy_from_slice(Impl::hash_bytes(&buf).as_bytes());
        out
    }
}

/// The `Gated` event's selector.
#[must_use]
pub fn gated_selector() -> [u8; 8] {
    let h = <RiscZeroSha as gate_core::Sha256>::hash(&[GATED_SELECTOR_NAME]);
    let mut out = [0_u8; 8];
    out.copy_from_slice(&h[..8]);
    out
}

/// The binding a fact must carry for `actor` at `nonce` in the consumer `consumer`.
#[must_use]
pub fn expected_binding(consumer: &AccountId, actor: &AccountId, nonce: u64) -> Bytes32 {
    let context = gate_core::context_onchain::<RiscZeroSha>(
        &ZONE_ID,
        GATE.value(),
        consumer.value(),
        &SUBMISSION_ID,
        actor.value(),
        nonce,
    );
    gate_core::binding::<RiscZeroSha>(NFT_PROGRAM.value(), COLLECTION.value(), &context)
}

/// The gate actor of a slot.
#[must_use]
pub fn slot_actor(slot: &Bytes32) -> Actor {
    Actor::new(AccountId::for_public_pda(&GATE, &PdaSeed::new(*slot)), GATE)
}

/// The guest entry point.
pub fn handle_message(input: &ReceiveInput) -> Response {
    let state = decode_state(&input.pre_state);
    if input.from.is_none() {
        return act(input, state);
    }
    if state.pending.is_some() {
        return gate_reply(input, state);
    }
    panic!("consumer: unexpected message")
}

fn act(input: &ReceiveInput, state: ConsumerState) -> Response {
    let ConsumerMessage::Act { slot } =
        ConsumerMessage::try_from_slice(&input.message).expect("consumer: message must decode");
    if !input.is_authorized {
        panic!("consumer: {}", Outcome::NotAuthorized.name());
    }
    assert!(state.pending.is_none(), "consumer: an Act is already pending");
    let next = ConsumerState { pending: Some(slot), ..state };
    Response::set_state(encode(&next)).send_sync(slot_actor(&slot), &GateMessage::ReadState)
}

fn gate_reply(input: &ReceiveInput, state: ConsumerState) -> Response {
    let slot = state.pending.expect("consumer: no Act is pending");
    assert!(
        input.from == Some(slot_actor(&slot)),
        "consumer: the reply is not from the slot's gate actor"
    );
    let reply = StateReply::try_from_slice(&input.message).expect("consumer: gate reply must decode");
    let actor = input.receiver.account_id;
    let expected = expected_binding(&input.receiver.program_account_id, &actor, state.nonce);
    // No clock on LEZ: the time rule is the sequencer's (the window set below).
    match gate_core::check_detailed(&reply.state, &expected, None) {
        (Outcome::Ok, Some(fact)) => {
            let event = Gated {
                submission_id: SUBMISSION_ID,
                slot,
                actor,
                nonce: state.nonce,
                expires_at: fact.expires_at,
            };
            let next = ConsumerState {
                nonce: state.nonce.checked_add(1).expect("consumer: nonce overflows"),
                pending: None,
            };
            Response::set_state(encode(&next))
                .event(ProgramEvent {
                    selector: gated_selector(),
                    data: borsh::to_vec(&event).expect("borsh serialization is infallible"),
                })
                .try_timestamp_window(fact.not_before..fact.expires_at)
                .expect("consumer: fact window is empty")
        }
        (code, _) => panic!("consumer: {}", code.name()),
    }
}

fn decode_state(bytes: &[u8]) -> ConsumerState {
    if bytes.is_empty() {
        return ConsumerState::default();
    }
    borsh::from_slice(bytes).expect("consumer: actor state must decode")
}

fn encode(state: &ConsumerState) -> Vec<u8> {
    borsh::to_vec(state).expect("borsh serialization is infallible")
}

#[cfg(test)]
mod tests;
