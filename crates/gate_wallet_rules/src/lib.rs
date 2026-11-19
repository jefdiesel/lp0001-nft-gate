// SPDX-License-Identifier: MIT OR Apache-2.0
//! The wallet's side of LP-0001 (spec §4.3-§4.6), with no LEZ dependency, so the CLI client
//! (`client/`) and the wallet module run the same rules and the rules are tested on their own.
//!
//! - [`plan`]: `not_before` and the TTL class from the clock and the request (§2.7).
//! - [`publish_message`]: the exact bytes of the one public write the gate may make.
//! - [`outcome_check`]: the fail-closed check of a proven transaction before it is sent (§4.4
//!   item 6). Anything unexpected stops the proof.
//! - [`WalletError`]: the plain, distinct errors of §4.5, and [`classify`] to map a refusal
//!   text from LEZ onto them.

use std::collections::BTreeMap;

use gate_core::{Bytes32, Fact, GateState, Sha2};

/// The native token program's account id: an account's balance is its actor state there
/// (LEZ `lee_core::native_token`; the all-zero id prints as `1111…1111`).
pub const NATIVE_TOKEN_PROGRAM: Bytes32 = [0; 32];

/// The fee reserve a private transaction held from its payer on the throwaway rc3 chain:
/// ~5,070,616, and one refusal named 5,171,104 (LP0001-JOB-A-FINDINGS.md §4, A2.4 case 3).
/// The client asks the wallet's own dry run for the real figure; this is the floor it shows.
pub const OBSERVED_RESERVE: u128 = 5_171_104;

/// Total memory a machine needs to prove (spec §1.2 row "consumer hardware": a 16 GB desktop
/// only; ~9.5 GB peak measured on bridge claims). macOS reports a 16 GB machine as 16 GiB.
pub const MIN_PROVING_MEMORY_BYTES: u64 = 15 * 1024 * 1024 * 1024;

/// The wallet's distinct errors (spec §4.5), plain and actionable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalletError {
    NotAMember,
    FeeReserve,
    AccountBusy,
    OutOfWindow,
    ProverOutOfMemory,
    RpcUnreachable,
    GateNotPinned,
    UnexpectedOutcome,
}

impl WalletError {
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::NotAMember => "NOT_A_MEMBER",
            Self::FeeReserve => "FEE_RESERVE",
            Self::AccountBusy => "ACCOUNT_BUSY",
            Self::OutOfWindow => "OUT_OF_WINDOW",
            Self::ProverOutOfMemory => "PROVER_OUT_OF_MEMORY",
            Self::RpcUnreachable => "RPC_UNREACHABLE",
            Self::GateNotPinned => "GATE_NOT_PINNED",
            Self::UnexpectedOutcome => "UNEXPECTED_OUTCOME",
        }
    }

    #[must_use]
    pub const fn plain(self) -> &'static str {
        match self {
            Self::NotAMember => "This account holds no NFT of that collection, so there is nothing to prove. Nothing was sent.",
            Self::FeeReserve => "The account paying the fee does not hold the fee reserve (about 5 million). Fund it first; that is one more private transaction.",
            Self::AccountBusy => "Your account changed while proving; try again.",
            Self::OutOfWindow => "The proof landed too late for its validity window; prove again.",
            Self::ProverOutOfMemory => "This computer does not have enough memory to make the proof (a 16 GB machine is needed).",
            Self::RpcUnreachable => "Could not reach the chain's RPC. Nothing was sent.",
            Self::GateNotPinned => "The gate or NFT program on chain is not the pinned, immutable one. Nothing was sent.",
            Self::UnexpectedOutcome => "The proven transaction would do something other than write one pass and pay the fee. It was stopped and not sent.",
        }
    }
}

/// Maps a refusal text from the wallet crate, the simulation or the sequencer onto a
/// [`WalletError`]. `None` when it is none of the known cases.
#[must_use]
pub fn classify(text: &str) -> Option<WalletError> {
    let t = text;
    if t.contains("UNEXPECTED_OUTCOME") {
        Some(WalletError::UnexpectedOutcome)
    } else if t.contains("NOT_A_MEMBER") {
        Some(WalletError::NotAMember)
    } else if t.contains("OutOfValidityWindow") || t.contains("validity window") {
        Some(WalletError::OutOfWindow)
    } else if t.contains("Nullifier already seen") || t.contains("nullifier") && t.contains("seen") {
        Some(WalletError::AccountBusy)
    } else if t.contains("Fee reserve") || t.contains("fee reserve") || t.contains("Can not pay") {
        Some(WalletError::FeeReserve)
    } else if t.contains("GATE_NOT_PINNED") {
        Some(WalletError::GateNotPinned)
    } else if t.contains("Connection refused") || t.contains("error trying to connect") || t.contains("RPC_UNREACHABLE") {
        Some(WalletError::RpcUnreachable)
    } else if t.contains("out of memory") || t.contains("memory allocation") {
        Some(WalletError::ProverOutOfMemory)
    } else {
        None
    }
}

/// A planned request: the window the wallet asks the gate for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    pub not_before: u64,
    pub ttl_class: u8,
    pub expires_at: u64,
}

/// The wallet rule (§2.7): `not_before = floor(now, QUANT_MS)`, the smallest TTL class that
/// covers `requested_ttl_ms`. `None` above the largest class.
#[must_use]
pub fn plan(now_ms: u64, requested_ttl_ms: u64) -> Option<Plan> {
    let not_before = gate_core::not_before_for(now_ms);
    let ttl_class = gate_core::ttl_class_for(requested_ttl_ms)?;
    let (not_before, expires_at) = gate_core::window(not_before, ttl_class)?;
    Some(Plan { not_before, ttl_class, expires_at })
}

/// The borsh bytes of `GateInternal::Publish { slot, fact }`: tag 0, slot, binding, the two
/// u64 little-endian. The only public write a correct `Prove` makes (checked against borsh in
/// the client's tests).
#[must_use]
pub fn publish_message(slot: &Bytes32, fact: &Fact) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 + 32 + 32 + 16);
    out.push(0);
    out.extend_from_slice(slot);
    out.extend_from_slice(&fact.binding);
    out.extend_from_slice(&fact.not_before.to_le_bytes());
    out.extend_from_slice(&fact.expires_at.to_le_bytes());
    out
}

/// An actor: (account, program).
pub type ActorId = (Bytes32, Bytes32);

/// One crossing of the private/public boundary in the proven transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    PrivateToPublic { to: ActorId, message: Vec<u8> },
    PublicToPrivate { to: ActorId, message: Vec<u8> },
    EndPrivateSubtree,
    EndPublicSubtree,
}

/// The public part of the proven transaction, as the client extracted it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedPublic {
    /// `execution.public_root.is_some()`: the root message would be published.
    pub public_root: bool,
    pub public_actors: Vec<ActorId>,
    pub boundary: Vec<Step>,
    /// The transaction's timestamp window `[from, to)`.
    pub window: (Option<u64>, Option<u64>),
    /// Asynchronous messages the transaction leaves pending (sealed).
    pub async_messages: usize,
}

/// One private account's actor states (program → bytes), before or after.
pub type States = BTreeMap<Bytes32, Vec<u8>>;

/// The private part, as the wallet's key chain sees it after following the proven
/// transaction (holder and payer only).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedPrivate {
    pub holder_before: States,
    pub holder_after: States,
    /// `None` when the holder pays its own fee.
    pub payer: Option<(States, States)>,
}

/// What a correct `Prove` from this holder must look like.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expected {
    pub gate: Bytes32,
    pub nft_program: Bytes32,
    pub slot: Bytes32,
    pub fact: Fact,
    /// The secret the holder's gate state must keep.
    pub secret: Bytes32,
    /// The fee recipient the wallet declares (the fee inbox).
    pub fee_inbox: Bytes32,
    /// The most the fee may take from the payer.
    pub max_fee: u128,
    /// Pending native credits to the holder that the wallet may claim before the root (the
    /// stock wallet sweeps up to 4, `lez/wallet/src/lib.rs` `sweepable`): its balance may rise
    /// by at most this.
    pub holder_credit: u128,
    /// The same for a separate payer.
    pub payer_credit: u128,
}

fn balance(states: &States) -> Option<u128> {
    match states.get(&NATIVE_TOKEN_PROGRAM) {
        None => Some(0),
        Some(b) if b.len() == 16 => {
            let mut buf = [0_u8; 16];
            buf.copy_from_slice(b);
            Some(u128::from_le_bytes(buf))
        }
        Some(_) => None,
    }
}

/// The fail-closed outcome check (§4.4 item 6). Every failure is listed; any failure means the
/// transaction must not be sent (`UNEXPECTED_OUTCOME`).
///
/// - The root message stays private (no public root): `slot_secret` is never published.
/// - Public actors: only the slot PDA's gate actor and the fee inbox's native actor.
/// - The boundary: exactly one write to `(slot PDA, gate)` with exactly the expected `Publish`
///   bytes, and exactly one fee credit to the fee inbox; nothing crosses back in.
/// - The window is exactly `[not_before, expires_at)`.
/// - No pending asynchronous message is left behind.
/// - The holder's NFT actor is byte-identical; its gate actor is `Holder { secret, pending:
///   None }`; no other actor state changes except the native balance.
/// - The native balance falls by at most `max_fee` on the payer only (holder or the separate
///   payer, whose other states do not change either).
pub fn outcome_check(e: &Expected, public: &ObservedPublic, private: &ObservedPrivate) -> Result<(), Vec<String>> {
    let mut bad = Vec::new();
    let pda = gate_core::slot_pda::<Sha2>(&e.gate, &e.slot);
    if public.public_root {
        bad.push("the root message would be public".to_owned());
    }
    for actor in &public.public_actors {
        if *actor != (pda, e.gate) && *actor != (e.fee_inbox, NATIVE_TOKEN_PROGRAM) {
            bad.push(format!("unexpected public actor {}/{}", hex(&actor.0), hex(&actor.1)));
        }
    }
    let expected_publish = publish_message(&e.slot, &e.fact);
    let mut writes = 0;
    let mut fees = 0;
    for step in &public.boundary {
        match step {
            Step::PrivateToPublic { to, message } if *to == (pda, e.gate) => {
                writes += 1;
                if *message != expected_publish {
                    bad.push("the slot write is not the expected fact".to_owned());
                }
            }
            Step::PrivateToPublic { to, .. } if *to == (e.fee_inbox, NATIVE_TOKEN_PROGRAM) => fees += 1,
            Step::PrivateToPublic { to, .. } => {
                bad.push(format!("a write to {}/{} besides the slot", hex(&to.0), hex(&to.1)));
            }
            Step::PublicToPrivate { to, .. } => {
                bad.push(format!("a public message into the private part ({}/{})", hex(&to.0), hex(&to.1)));
            }
            Step::EndPrivateSubtree | Step::EndPublicSubtree => {}
        }
    }
    if writes != 1 {
        bad.push(format!("{writes} slot writes, expected exactly 1"));
    }
    if fees != 1 {
        bad.push(format!("{fees} fee credits, expected exactly 1"));
    }
    if public.window != (Some(e.fact.not_before), Some(e.fact.expires_at)) {
        bad.push(format!("window {:?}, expected [{}, {})", public.window, e.fact.not_before, e.fact.expires_at));
    }
    if public.async_messages != 0 {
        bad.push(format!("{} pending asynchronous message(s) left behind", public.async_messages));
    }

    // The holder.
    let hb = &private.holder_before;
    let ha = &private.holder_after;
    if hb.get(&e.nft_program) != ha.get(&e.nft_program) {
        bad.push("the holder's NFT actor changed".to_owned());
    }
    match ha.get(&e.gate).map(|b| GateState::decode(b)) {
        Some(Ok(GateState::Holder { secret, pending: None })) if secret == e.secret => {}
        other => bad.push(format!("the holder's gate state after is {other:?}, expected an idle Holder with its secret")),
    }
    let programs: std::collections::BTreeSet<&Bytes32> = hb.keys().chain(ha.keys()).collect();
    for p in programs {
        if *p == e.gate || *p == e.nft_program || *p == NATIVE_TOKEN_PROGRAM {
            continue;
        }
        if hb.get(p) != ha.get(p) {
            bad.push(format!("the holder's actor under {} changed", hex(p)));
        }
    }
    // The payer's balance: at most the fee lower, at most its claimed credits higher
    // (b - max_fee <= a <= b + credit). Anyone else's: never lower, at most its credits higher.
    let fee_check = |before: &States, after: &States, credit: u128, who: &str, bad: &mut Vec<String>| match (balance(before), balance(after)) {
        (Some(b), Some(a)) if a <= b.saturating_add(credit) && a >= b.saturating_sub(e.max_fee) => {}
        (b, a) => bad.push(format!(
            "{who}'s balance {b:?} -> {a:?} is not a fee of at most {} (claimable credits {credit})",
            e.max_fee
        )),
    };
    match &private.payer {
        None => fee_check(hb, ha, e.holder_credit, "the holder", &mut bad),
        Some((pb, pa)) => {
            match (balance(hb), balance(ha)) {
                (Some(b), Some(a)) if a >= b && a <= b.saturating_add(e.holder_credit) => {}
                _ => bad.push("the holder's balance changed though another account pays".to_owned()),
            }
            fee_check(pb, pa, e.payer_credit, "the payer", &mut bad);
            let programs: std::collections::BTreeSet<&Bytes32> = pb.keys().chain(pa.keys()).collect();
            for p in programs {
                if *p != NATIVE_TOKEN_PROGRAM && pb.get(p) != pa.get(p) {
                    bad.push(format!("the payer's actor under {} changed", hex(p)));
                }
            }
        }
    }
    if bad.is_empty() { Ok(()) } else { Err(bad) }
}

/// Pre-check 1 (§4.4): enough memory to prove. `total_bytes` from the OS.
#[must_use]
pub const fn enough_memory(total_bytes: u64) -> bool {
    total_bytes >= MIN_PROVING_MEMORY_BYTES
}

/// Pre-check 2 (§4.4, §4.3): the payer covers the reserve alone, so the fee fallback is always
/// present and the transaction size does not vary.
#[must_use]
pub const fn covers_reserve(payer_balance: u128, reserve: u128) -> bool {
    payer_balance >= reserve
}

/// Pre-check 4 (§4.4): the gate on chain is the pinned, immutable image and serves the pinned
/// NFT program.
pub fn gate_is_pinned(
    on_chain_image: &[u32; 8],
    on_chain_immutable: bool,
    pinned_image: &[u32; 8],
    gate_nft_program: &Bytes32,
    pinned_nft_program: &Bytes32,
) -> Result<(), String> {
    if on_chain_image != pinned_image {
        return Err("the gate's image id is not the pinned one".to_owned());
    }
    if !on_chain_immutable {
        return Err("the gate's header is not immutable".to_owned());
    }
    if gate_nft_program != pinned_nft_program {
        return Err("the gate serves another NFT program than the pinned one".to_owned());
    }
    Ok(())
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[cfg(test)]
mod tests;
