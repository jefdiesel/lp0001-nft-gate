// SPDX-License-Identifier: MIT OR Apache-2.0
//! The LP-0001 gate program (spec §2): one private `Prove`, rooted on the holder's own gate
//! actor, reads the holder's own actor of the pinned reference NFT program through that
//! program's `ReadState` and writes one opaque public fact to the holder's slot.
//!
//! It writes no NFT state and sends the NFT program nothing but `ReadState`.
//!
//! Dispatch, in this order, so the four kinds of input cannot be confused (spec §2.6):
//! 1. `input.from == None`: a transaction root. Only `Prove`.
//! 2. `input.from_own_program()`: `Publish`, at the slot PDA only.
//! 3. The NFT reply: state `Holder` with a pending proof, and the sender is exactly
//!    `(own account, NFT_PROGRAM)`.
//! 4. Any other program: `ReadState` of a fact or an empty actor. Everything else panics.

use borsh::BorshDeserialize as _;
use gate_core::{Bytes32, Fact, GateState, Pending, Sha256, HOLDER_LEN_PENDING};
use gate_interface::{FactFields, GateInternal, GateMessage};
use lee_core::{
    account::{AccountId, Actor},
    program::{PdaSeed, ReceiveInput, Response, StateReply},
};
use nft_core::{Message as NftMessage, NftState};

/// The account id of the reference NFT program this gate serves. A deploy constant, fixed
/// forever by the immutable header (OWNER DECISION Q1: the NFT program is pinned in the
/// gate). Set `LP0001_NFT_PROGRAM` (64 hex) when building the guest; native tests use
/// `[0xAA; 32]`, as the NFT program's own test fixtures do.
pub const NFT_PROGRAM: AccountId =
    AccountId::new(gate_core::deploy_constant(option_env!("LP0001_NFT_PROGRAM"), [0xAA; 32]));

// A guest build without the constant would pin the test value: refuse to build.
#[cfg(target_os = "zkvm")]
const _: () = assert!(
    option_env!("LP0001_NFT_PROGRAM").is_some(),
    "set LP0001_NFT_PROGRAM (64 hex) to the reference NFT program's account id"
);

/// SHA-256 through `risc0_zkvm::sha`, as probe v2 does; accelerated inside the guest.
pub struct RiscZeroSha;

impl Sha256 for RiscZeroSha {
    fn hash(parts: &[&[u8]]) -> Bytes32 {
        use risc0_zkvm::sha::{Impl, Sha256 as _};
        let len = parts.iter().map(|p| p.len()).sum();
        let mut buf = Vec::with_capacity(len);
        for part in parts {
            buf.extend_from_slice(part);
        }
        let digest = Impl::hash_bytes(&buf);
        let mut out = [0_u8; 32];
        out.copy_from_slice(digest.as_bytes());
        out
    }
}

/// The slot PDA's account id, through LEE's own derivation.
#[must_use]
pub fn slot_account(gate: &AccountId, slot: &Bytes32) -> AccountId {
    AccountId::for_public_pda(gate, &PdaSeed::new(*slot))
}

/// The guest entry point.
pub fn handle_message(input: &ReceiveInput) -> Response {
    // 1. A transaction root.
    if input.from.is_none() {
        return root(input);
    }
    // 2. From the gate's own program: only `Publish`.
    if input.from_own_program() {
        return publish(input);
    }
    let state = decode_state(&input.pre_state);
    // 3. The reply of the pinned NFT program's actor on this same account.
    if let Some(GateState::Holder { secret, pending: Some(pending) }) = state {
        if input.from == Some(Actor::new(input.receiver.account_id, NFT_PROGRAM)) {
            return nft_reply(input, secret, pending, NFT_PROGRAM);
        }
        #[cfg(feature = "unsafe-test-gate")]
        if let Some(from) = input.from {
            if from.account_id == input.receiver.account_id {
                // TEST ONLY: the caller-named "NFT program" answers.
                return nft_reply(input, secret, pending, from.program_account_id);
            }
        }
    }
    // 4. Any other program.
    other_program(input, state)
}

fn root(input: &ReceiveInput) -> Response {
    let message =
        GateMessage::try_from_slice(&input.message).expect("gate: message must decode");
    match message {
        GateMessage::Prove { collection, context, not_before, ttl_class, slot_secret } => {
            assert!(input.is_authorized, "gate: root not authorized");
            prove(input, NFT_PROGRAM, collection, context, not_before, ttl_class, slot_secret)
        }
        GateMessage::ReadState => panic!("gate: ReadState is refused as a transaction root"),
        GateMessage::TestProve { nft_program, collection, context, not_before, ttl_class, slot_secret } => {
            test_prove(input, nft_program, collection, context, not_before, ttl_class, slot_secret)
        }
    }
}

#[cfg(not(feature = "unsafe-test-gate"))]
fn test_prove(
    _: &ReceiveInput,
    _: AccountId,
    _: AccountId,
    _: [u8; 32],
    _: u64,
    _: u8,
    _: Option<[u8; 32]>,
) -> Response {
    panic!("gate: TestProve exists only in the unsafe test gate")
}

/// TEST ONLY: no authorisation assert, caller-named NFT program (tests T3 control, T4).
#[cfg(feature = "unsafe-test-gate")]
fn test_prove(
    input: &ReceiveInput,
    nft_program: AccountId,
    collection: AccountId,
    context: [u8; 32],
    not_before: u64,
    ttl_class: u8,
    slot_secret: Option<[u8; 32]>,
) -> Response {
    prove(input, nft_program, collection, context, not_before, ttl_class, slot_secret)
}

#[allow(clippy::too_many_arguments, reason = "the message's fields")]
fn prove(
    input: &ReceiveInput,
    nft_program: AccountId,
    collection: AccountId,
    context: [u8; 32],
    not_before: u64,
    ttl_class: u8,
    slot_secret: Option<[u8; 32]>,
) -> Response {
    let secret = match (decode_state(&input.pre_state), slot_secret) {
        (None, Some(secret)) => secret,
        (None, None) => panic!("gate: a first proof needs a slot secret"),
        (Some(GateState::Holder { pending: Some(_), .. }), _) => {
            panic!("gate: a proof is already pending")
        }
        (Some(GateState::Holder { secret, pending: None }), None) => secret,
        (Some(GateState::Holder { .. }), Some(_)) => panic!("gate: the slot secret is already set"),
        (Some(GateState::Fact(_)), _) => panic!("gate: a slot cannot prove"),
    };
    assert!(not_before % gate_core::QUANT_MS == 0, "gate: not_before is off the grid");
    let ttl = *gate_core::TTL_MS
        .get(usize::from(ttl_class))
        .unwrap_or_else(|| panic!("gate: bad ttl_class"));
    let expires_at = not_before.checked_add(ttl).expect("gate: expiry overflows");
    let binding = gate_core::binding::<RiscZeroSha>(nft_program.value(), collection.value(), &context);
    let holder = GateState::Holder {
        secret,
        pending: Some(Pending { collection: *collection.value(), binding, not_before, expires_at }),
    };
    // The read target is this gate actor's own account and the pinned NFT program: neither is
    // a parameter, so the asker is the holder by construction.
    Response::set_state(encode(&holder))
        .send_sync(Actor::new(input.receiver.account_id, nft_program), &NftMessage::ReadState)
        .try_timestamp_window(not_before..expires_at)
        .expect("gate: the window is empty")
}

fn nft_reply(input: &ReceiveInput, secret: Bytes32, pending: Pending, _nft: AccountId) -> Response {
    #[cfg(not(feature = "unsafe-test-gate"))]
    assert!(input.is_authorized, "gate: reply not authorized");
    let reply = StateReply::try_from_slice(&input.message).expect("gate: NFT reply must decode");
    // `NftState::decode` panics on bad bytes; decode with no panic path instead.
    let nft_state: Option<NftState> = if reply.state.is_empty() {
        None
    } else {
        borsh::from_slice(&reply.state).ok()
    };
    let collection = AccountId::new(pending.collection);
    let member = match nft_state {
        Some(NftState::Copy(copy)) => copy.definition_id == collection && copy.owned,
        Some(NftState::Master(master)) => {
            gate_core::MASTER_COUNTS && master.definition_id == collection && master.print_balance >= 1
        }
        Some(NftState::Definition(_)) | None => false,
    };
    assert!(member, "gate: NOT_A_MEMBER");
    let slot = gate_core::slot::<RiscZeroSha>(&secret);
    let gate = input.receiver.program_account_id;
    let fact = Fact { binding: pending.binding, not_before: pending.not_before, expires_at: pending.expires_at };
    Response::set_state(encode(&GateState::Holder { secret, pending: None })).send_sync(
        Actor::new(slot_account(&gate, &slot), gate),
        &GateInternal::Publish { slot, fact: FactFields::from(fact) },
    )
}

fn publish(input: &ReceiveInput) -> Response {
    let GateInternal::Publish { slot, fact } =
        GateInternal::try_from_slice(&input.message).expect("gate: internal message must decode");
    assert!(
        input.receiver.account_id == slot_account(&input.receiver.program_account_id, &slot),
        "gate: Publish reached an actor that is not its slot"
    );
    match decode_state(&input.pre_state) {
        None | Some(GateState::Fact(_)) => {}
        Some(GateState::Holder { .. }) => panic!("gate: Publish reached a holder"),
    }
    // Overwrites whatever fact was there: a newer proof invalidates the older.
    Response::set_state(encode(&GateState::Fact(Fact::from(fact))))
}

fn other_program(input: &ReceiveInput, state: Option<GateState>) -> Response {
    let message =
        GateMessage::try_from_slice(&input.message).expect("gate: message must decode");
    match message {
        GateMessage::ReadState => match state {
            None | Some(GateState::Fact(_)) => {
                let reader = input.from.expect("gate: ReadState needs a sender");
                Response::keep_state().send_sync(reader, &StateReply::from(input))
            }
            Some(GateState::Holder { .. }) => panic!("gate: a holder's state is not readable"),
        },
        GateMessage::Prove { .. } | GateMessage::TestProve { .. } => {
            panic!("gate: Prove is accepted only as a transaction root")
        }
    }
}

/// Decodes the actor state: `None` when empty; panics on bytes that are not a gate state
/// (only gate code writes gate state).
fn decode_state(bytes: &[u8]) -> Option<GateState> {
    if bytes.is_empty() {
        return None;
    }
    Some(GateState::decode(bytes).expect("gate: actor state must decode"))
}

fn encode(state: &GateState) -> Vec<u8> {
    let mut buf = [0_u8; HOLDER_LEN_PENDING];
    let len = state.encode_into(&mut buf);
    buf[..len].to_vec()
}

#[cfg(test)]
mod tests;
