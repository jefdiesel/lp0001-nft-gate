// SPDX-License-Identifier: MIT OR Apache-2.0
//! The wire types of the LP-0001 gate and its reference consumer (spec §2.5, §3.1).
//!
//! State and messages outlive code, and borsh encodes an enum variant as a leading tag byte,
//! so every enum here is append-only, as in the reference NFT program.
//!
//! The gate's *actor state* is not here: it is `gate_core::GateState`, hand-encoded to the
//! same bytes borsh would produce (checked in `gate_program`'s tests), so the off-chain
//! library can read it with no borsh and no LEZ types.

use borsh::{BorshDeserialize, BorshSerialize};
use lee_core::account::AccountId;

/// Messages to the gate.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum GateMessage {
    /// Transaction root only, on the holder's own gate actor. The holder must authorise it.
    ///
    /// `slot_secret` is `Some` on the holder's first proof only (it becomes the secret kept
    /// in the shielded gate state) and `None` after.
    Prove {
        collection: AccountId,
        context: [u8; 32],
        not_before: u64,
        ttl_class: u8,
        slot_secret: Option<[u8; 32]>,
    },
    /// From another program only: reply with this actor's state as a `StateReply`, when it is
    /// a fact or empty. Same convention as the NFT program's `ReadState`.
    ReadState,
    /// TEST ONLY (feature `unsafe-test-gate` of `gate_program`; refused by the real gate):
    /// `Prove` with a caller-named NFT program and no authorisation assert. Tests T3 and T4.
    TestProve {
        nft_program: AccountId,
        collection: AccountId,
        context: [u8; 32],
        not_before: u64,
        ttl_class: u8,
        slot_secret: Option<[u8; 32]>,
    },
}

/// Messages only the gate's own actors send each other.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum GateInternal {
    /// To the gate actor of the slot PDA `for_public_pda(gate, PdaSeed(slot))`: write `fact`.
    Publish { slot: [u8; 32], fact: FactFields },
}

/// The fact's fields on the wire (the state itself is `gate_core::Fact`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FactFields {
    pub binding: [u8; 32],
    pub not_before: u64,
    pub expires_at: u64,
}

impl From<gate_core::Fact> for FactFields {
    fn from(f: gate_core::Fact) -> Self {
        Self { binding: f.binding, not_before: f.not_before, expires_at: f.expires_at }
    }
}

impl From<FactFields> for gate_core::Fact {
    fn from(f: FactFields) -> Self {
        Self { binding: f.binding, not_before: f.not_before, expires_at: f.expires_at }
    }
}

/// Messages to the reference consumer.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum ConsumerMessage {
    /// Transaction root on `(A, consumer)`, authorised by A: gate the action on the fact in
    /// `slot`, for A at A's current consumer nonce.
    Act { slot: [u8; 32] },
}

/// The consumer's state on the acting public account A.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ConsumerState {
    /// A's next nonce in this consumer: the one the next on-chain context must name.
    pub nonce: u64,
    /// The slot an `Act` in this transaction is waiting on the gate for.
    pub pending: Option<[u8; 32]>,
}

/// The event of one gated action. Carries the submission id the prize asks for.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Gated {
    pub submission_id: [u8; 32],
    pub slot: [u8; 32],
    pub actor: AccountId,
    /// The nonce that was in the context (the state's nonce before the bump).
    pub nonce: u64,
    pub expires_at: u64,
}

/// The `Gated` event selector: the first 8 bytes of `sha256("lp0001-consumer::Gated")`,
/// the LEZ convention (`lee_core::program::ProgramEvent`).
pub const GATED_SELECTOR_NAME: &[u8] = b"lp0001-consumer::Gated";

/// 32 bytes of an account id.
#[must_use]
pub fn id_bytes(id: &AccountId) -> [u8; 32] {
    *id.value()
}
