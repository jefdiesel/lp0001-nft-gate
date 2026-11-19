// SPDX-License-Identifier: MIT OR Apache-2.0
//! TEST ONLY (T4): an attacker stand-in. On `ReadState` from the unsafe test gate it sends the
//! reference NFT program's `Transfer { to: ATTACKER }` to the holder's real NFT actor, then
//! answers the gate with a forged owned copy of COLLECTION.
//!
//! Constants (64 hex each, at build time): LP0001_T4_REAL_NFT, LP0001_T4_ATTACKER,
//! LP0001_T4_COLLECTION.

use lee_core::{
    account::{AccountId, Actor, ActorState},
    program::{ReceiveInput, Response, StateReply},
};
use nft_core::{Message as NftMessage, NftCopy, NftState};

pub const REAL_NFT: AccountId = AccountId::new(gate_core::deploy_constant(option_env!("LP0001_T4_REAL_NFT"), [0xAA; 32]));
pub const ATTACKER: AccountId = AccountId::new(gate_core::deploy_constant(option_env!("LP0001_T4_ATTACKER"), [0xEE; 32]));
pub const COLLECTION: AccountId = AccountId::new(gate_core::deploy_constant(option_env!("LP0001_T4_COLLECTION"), [0xC0; 32]));

pub fn handle_message(input: &ReceiveInput) -> Response {
    let asker = input.from.expect("hostile: needs a sender");
    let message: NftMessage = borsh::from_slice(&input.message).expect("hostile: message must decode");
    assert!(matches!(message, NftMessage::ReadState), "hostile: only ReadState");
    let forged = NftState::Copy(NftCopy { definition_id: COLLECTION, owned: true }).encode();
    Response::keep_state()
        // With the holder's authority (authority is per account, not per program):
        .send_sync(Actor::new(input.receiver.account_id, REAL_NFT), &NftMessage::Transfer { to: ATTACKER })
        .send_sync(asker, &StateReply { state: ActorState::from(forged) })
}
