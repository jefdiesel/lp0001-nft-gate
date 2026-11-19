// SPDX-License-Identifier: MIT OR Apache-2.0
//! `gate_core`: the shared, dependency-free core of the LP-0001 ownership gate.
//!
//! One function decides every verdict, [`check`]. The gate guest, the consumer guest, the
//! native verifier library and the wasm build all call it over the same bytes, so the
//! on-chain and off-chain verifiers cannot disagree about a fact.
//!
//! What lives here (spec `LP0001-OWNERSHIP-PROOF-SPEC.md` §2.2-§2.4, §3.3):
//! - the gate's actor state layout ([`GateState`], [`Fact`], [`Pending`]), encoded byte for
//!   byte as borsh encodes the same Rust types (checked in `gate_program`'s tests);
//! - the derivations: [`slot`], [`slot_pda`], [`context`], [`binding`], and the on-chain and
//!   off-chain payloads;
//! - the time grid and TTL menu ([`QUANT_MS`], [`TTL_MS`]) and the wallet's time rule;
//! - the nine outcome codes ([`Outcome`]).
//!
//! No LEZ dependency. SHA-256 comes from the caller through [`Sha256`], so the guest hashes
//! with `risc0_zkvm::sha` and everything else with the `sha2` crate (feature `sha2`).
#![no_std]
#![forbid(unsafe_code)]

/// A 32-byte value: an account id, a hash, a secret.
pub type Bytes32 = [u8; 32];

// ---------------------------------------------------------------------------------------------
// Deploy constants. OWNER DECISION Q1 (spec §9.1): these are fixed forever once the gate is
// deployed with an immutable header. The values are the spec's stated defaults.
// ---------------------------------------------------------------------------------------------

/// Grid for `not_before`, in milliseconds. OWNER DECISION Q1, default 10 minutes.
pub const QUANT_MS: u64 = 600_000;

/// The validity menu, in milliseconds: 1 h, 6 h, 24 h. OWNER DECISION Q1 (default).
pub const TTL_MS: [u64; 3] = [3_600_000, 21_600_000, 86_400_000];

/// The largest TTL.
pub const MAX_TTL_MS: u64 = TTL_MS[2];

/// Whether a master (`NftState::Master`, print balance >= 1) counts as holding the
/// collection. OWNER DECISION Q2 (spec §9.2), default: yes.
pub const MASTER_COUNTS: bool = true;

/// Domain string of the slot derivation (spec §2.1).
pub const DOMAIN_SLOT: &[u8] = b"lp0001-gate/v1/slot";
/// Domain string of the context derivation (spec §2.1).
pub const DOMAIN_CTX: &[u8] = b"lp0001-gate/v1/ctx";
/// Domain string of the binding derivation (spec §2.1).
pub const DOMAIN_BIND: &[u8] = b"lp0001-gate/v1/bind";

/// LEE's public-PDA prefix, `lee_core::program::AccountId::for_public_pda`
/// (LEZ v0.4.0-rc3 `lee/state_machine/core/src/program/mod.rs:154-155`).
pub const LEE_PDA_PREFIX: &[u8; 32] = b"/LEE/v0.2/AccountId/PDA/\x00\x00\x00\x00\x00\x00\x00\x00";

/// Context kind byte: a context consumed by a LEZ program (spec §2.4).
pub const KIND_ONCHAIN: u8 = 0x01;
/// Context kind byte: a context checked in application code (spec §2.4).
pub const KIND_OFFCHAIN: u8 = 0x02;

/// Version byte inside every gate state.
pub const STATE_VERSION: u8 = 1;

/// Borsh enum tag of `GateState::Holder`.
pub const TAG_HOLDER: u8 = 0;
/// Borsh enum tag of `GateState::Fact`.
pub const TAG_FACT: u8 = 1;

/// Length of an encoded `GateState::Fact`: tag, version, binding, two u64 (spec §2.2: 50).
pub const FACT_LEN: usize = 1 + 1 + 32 + 8 + 8;

/// Parses 64 hex characters at compile time, for deploy constants given as environment
/// variables (`option_env!`). Panics (a compile error in a `const`) on anything else.
#[must_use]
pub const fn hex32(s: &str) -> Bytes32 {
    const fn nibble(c: u8) -> u8 {
        match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            b'A'..=b'F' => c - b'A' + 10,
            _ => panic!("deploy constant: not a hex character"),
        }
    }
    let b = s.as_bytes();
    assert!(b.len() == 64, "deploy constant: expected 64 hex characters");
    let mut out = [0_u8; 32];
    let mut i = 0;
    while i < 32 {
        out[i] = (nibble(b[2 * i]) << 4) | nibble(b[2 * i + 1]);
        i += 1;
    }
    out
}

/// A deploy constant from an environment variable at build time, or `default` when it is
/// unset (native tests). Guests refuse to build without it (see each program crate).
#[must_use]
pub const fn deploy_constant(value: Option<&str>, default: Bytes32) -> Bytes32 {
    match value {
        Some(s) => hex32(s),
        None => default,
    }
}

// ---------------------------------------------------------------------------------------------
// Hashing
// ---------------------------------------------------------------------------------------------

/// SHA-256 over the concatenation of `parts`. Implemented by the caller.
pub trait Sha256 {
    fn hash(parts: &[&[u8]]) -> Bytes32;
}

/// The `sha2` crate's SHA-256, for everything that is not a risc0 guest.
#[cfg(feature = "sha2")]
pub struct Sha2;

#[cfg(feature = "sha2")]
impl Sha256 for Sha2 {
    fn hash(parts: &[&[u8]]) -> Bytes32 {
        use sha2::Digest as _;
        let mut hasher = sha2::Sha256::new();
        for part in parts {
            hasher.update(part);
        }
        hasher.finalize().into()
    }
}

// ---------------------------------------------------------------------------------------------
// Derivations (spec §2.3, §2.4)
// ---------------------------------------------------------------------------------------------

/// `slot = sha256("lp0001-gate/v1/slot" ‖ secret)`. The secret lives only in the holder's
/// shielded gate state.
///
/// OWNER DECISION Q4 (spec §9.4), default: one slot per holding. Keying per holding and app
/// would add the audience here; it must be decided before deploy.
#[must_use]
pub fn slot<H: Sha256>(secret: &Bytes32) -> Bytes32 {
    H::hash(&[DOMAIN_SLOT, secret])
}

/// The slot's public account: `AccountId::for_public_pda(gate, PdaSeed(slot))`, reimplemented
/// byte for byte (`sha256(LEE_PDA_PREFIX ‖ gate ‖ slot)`).
#[must_use]
pub fn slot_pda<H: Sha256>(gate: &Bytes32, slot: &Bytes32) -> Bytes32 {
    H::hash(&[LEE_PDA_PREFIX, gate, slot])
}

/// `context = sha256("lp0001-gate/v1/ctx" ‖ kind ‖ zone_id ‖ gate ‖ audience ‖ payload)`.
#[must_use]
pub fn context<H: Sha256>(
    kind: u8,
    zone_id: &Bytes32,
    gate: &Bytes32,
    audience: &Bytes32,
    payload: &Bytes32,
) -> Bytes32 {
    H::hash(&[DOMAIN_CTX, &[kind], zone_id, gate, audience, payload])
}

/// `binding = sha256("lp0001-gate/v1/bind" ‖ nft_program ‖ collection ‖ context)`.
#[must_use]
pub fn binding<H: Sha256>(nft_program: &Bytes32, collection: &Bytes32, context: &Bytes32) -> Bytes32 {
    H::hash(&[DOMAIN_BIND, nft_program, collection, context])
}

/// On-chain payload: `sha256(SUBMISSION_ID ‖ acting account A ‖ A's nonce in the consumer)`.
///
/// Encoding (the spec does not fix it; chosen to match borsh): `SUBMISSION_ID` is 32 bytes,
/// the nonce is a `u64` in little-endian.
#[must_use]
pub fn payload_onchain<H: Sha256>(submission_id: &Bytes32, actor: &Bytes32, nonce: u64) -> Bytes32 {
    H::hash(&[submission_id, actor, &nonce.to_le_bytes()])
}

/// Off-chain payload: `sha256(purpose ‖ nonce)`, the nonce being the app's 32 random bytes.
///
/// Encoding (the spec does not fix it): `purpose` is the UTF-8 bytes of the app's purpose
/// string, unprefixed; the fixed-length nonce comes last, so the split is unambiguous.
#[must_use]
pub fn payload_offchain<H: Sha256>(purpose: &[u8], nonce: &Bytes32) -> Bytes32 {
    H::hash(&[purpose, nonce])
}

/// Off-chain audience: a hash of the dApp identity as the wallet's provider sees the caller.
///
/// Encoding (the spec does not fix it): `sha256(UTF-8 caller identity)`. The wallet fills it
/// in; the dApp cannot set it. Which identifier Basecamp gives the provider is UNVERIFIED.
#[must_use]
pub fn audience_offchain<H: Sha256>(caller_identity: &[u8]) -> Bytes32 {
    H::hash(&[caller_identity])
}

/// The on-chain context a consumer program rebuilds (spec §3.1): audience = the consumer's
/// own program id, payload over the acting account and its consumer nonce.
#[must_use]
pub fn context_onchain<H: Sha256>(
    zone_id: &Bytes32,
    gate: &Bytes32,
    consumer: &Bytes32,
    submission_id: &Bytes32,
    actor: &Bytes32,
    nonce: u64,
) -> Bytes32 {
    let payload = payload_onchain::<H>(submission_id, actor, nonce);
    context::<H>(KIND_ONCHAIN, zone_id, gate, consumer, &payload)
}

/// The off-chain context an app rebuilds from its own values (spec §3.2 step 1).
#[must_use]
pub fn context_offchain<H: Sha256>(
    zone_id: &Bytes32,
    gate: &Bytes32,
    caller_identity: &[u8],
    purpose: &[u8],
    nonce: &Bytes32,
) -> Bytes32 {
    let audience = audience_offchain::<H>(caller_identity);
    let payload = payload_offchain::<H>(purpose, nonce);
    context::<H>(KIND_OFFCHAIN, zone_id, gate, &audience, &payload)
}

// ---------------------------------------------------------------------------------------------
// Time (spec §2.7)
// ---------------------------------------------------------------------------------------------

/// The wallet rule: `not_before = floor(now, QUANT_MS)`.
#[must_use]
pub const fn not_before_for(now_ms: u64) -> u64 {
    now_ms - now_ms % QUANT_MS
}

/// The wallet rule: the smallest TTL class that covers `requested_ms`; `None` above MAX_TTL.
#[must_use]
pub fn ttl_class_for(requested_ms: u64) -> Option<u8> {
    let mut class = 0_u8;
    for ttl in TTL_MS {
        if ttl >= requested_ms {
            return Some(class);
        }
        class += 1;
    }
    None
}

/// `expires_at = not_before + TTL_MS[ttl_class]` with every rule the gate asserts: the grid,
/// the class, no overflow. `None` where the gate would panic.
#[must_use]
pub fn window(not_before: u64, ttl_class: u8) -> Option<(u64, u64)> {
    if not_before % QUANT_MS != 0 {
        return None;
    }
    let ttl = *TTL_MS.get(usize::from(ttl_class))?;
    let expires_at = not_before.checked_add(ttl)?;
    Some((not_before, expires_at))
}

// ---------------------------------------------------------------------------------------------
// State layout (spec §2.2)
// ---------------------------------------------------------------------------------------------

/// The one public fact on a slot PDA's gate actor. 50 bytes with the tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fact {
    pub binding: Bytes32,
    pub not_before: u64,
    pub expires_at: u64,
}

/// A proof in flight, kept on the holder's shielded gate actor across the NFT reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pending {
    pub collection: Bytes32,
    pub binding: Bytes32,
    pub not_before: u64,
    pub expires_at: u64,
}

/// Every gate actor's state. An empty actor state is "nothing yet" and is not a `GateState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateState {
    /// On the gate actor of the holder's private account. Shielded.
    Holder { secret: Bytes32, pending: Option<Pending> },
    /// On the gate actor of a slot PDA. Public.
    Fact(Fact),
}

/// Why bytes are not a gate state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    Empty,
    BadLength,
    BadTag,
    BadVersion,
    BadOption,
    /// Times the gate cannot write: off the grid, or not one TTL apart.
    BadTimes,
}

impl Fact {
    /// The borsh bytes of `GateState::Fact { version: 1, binding, not_before, expires_at }`.
    #[must_use]
    pub fn encode(&self) -> [u8; FACT_LEN] {
        let mut out = [0_u8; FACT_LEN];
        out[0] = TAG_FACT;
        out[1] = STATE_VERSION;
        out[2..34].copy_from_slice(&self.binding);
        out[34..42].copy_from_slice(&self.not_before.to_le_bytes());
        out[42..50].copy_from_slice(&self.expires_at.to_le_bytes());
        out
    }

    /// Strict decode of a version-1 `Fact` exactly as the gate writes it: exactly 50 bytes,
    /// tag and version right, `not_before` on the grid and `expires_at - not_before` one of
    /// the TTL menu. Anything else is not a fact the gate can have written.
    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        if bytes.is_empty() {
            return Err(DecodeError::Empty);
        }
        if bytes.len() != FACT_LEN {
            return Err(DecodeError::BadLength);
        }
        if bytes[0] != TAG_FACT {
            return Err(DecodeError::BadTag);
        }
        if bytes[1] != STATE_VERSION {
            return Err(DecodeError::BadVersion);
        }
        let mut binding = [0_u8; 32];
        binding.copy_from_slice(&bytes[2..34]);
        let not_before = read_u64(&bytes[34..42]);
        let expires_at = read_u64(&bytes[42..50]);
        let fact = Self { binding, not_before, expires_at };
        if !fact.is_gate_shaped() {
            return Err(DecodeError::BadTimes);
        }
        Ok(fact)
    }

    /// Whether the times are ones the gate can write: on the grid and one TTL apart.
    #[must_use]
    pub fn is_gate_shaped(&self) -> bool {
        self.not_before % QUANT_MS == 0
            && self
                .expires_at
                .checked_sub(self.not_before)
                .is_some_and(|ttl| TTL_MS.contains(&ttl))
    }
}

const PENDING_LEN: usize = 32 + 32 + 8 + 8;
/// Length of an encoded `Holder` with no pending proof.
pub const HOLDER_LEN_IDLE: usize = 1 + 1 + 32 + 1;
/// Length of an encoded `Holder` with a pending proof.
pub const HOLDER_LEN_PENDING: usize = HOLDER_LEN_IDLE + PENDING_LEN;

impl GateState {
    /// Encodes as borsh would: tag, version, fields; `Option` as 0 / 1 + value.
    /// Writes into `out` and returns the length used (at most [`HOLDER_LEN_PENDING`]).
    pub fn encode_into(&self, out: &mut [u8; HOLDER_LEN_PENDING]) -> usize {
        match self {
            Self::Fact(fact) => {
                out[..FACT_LEN].copy_from_slice(&fact.encode());
                FACT_LEN
            }
            Self::Holder { secret, pending } => {
                out[0] = TAG_HOLDER;
                out[1] = STATE_VERSION;
                out[2..34].copy_from_slice(secret);
                match pending {
                    None => {
                        out[34] = 0;
                        HOLDER_LEN_IDLE
                    }
                    Some(p) => {
                        out[34] = 1;
                        out[35..67].copy_from_slice(&p.collection);
                        out[67..99].copy_from_slice(&p.binding);
                        out[99..107].copy_from_slice(&p.not_before.to_le_bytes());
                        out[107..115].copy_from_slice(&p.expires_at.to_le_bytes());
                        HOLDER_LEN_PENDING
                    }
                }
            }
        }
    }

    /// Strict decode of any gate state. A `Fact` decodes as in [`Fact::decode`].
    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        match bytes.first() {
            None => Err(DecodeError::Empty),
            Some(&TAG_FACT) => Fact::decode(bytes).map(Self::Fact),
            Some(&TAG_HOLDER) => {
                if bytes.len() < HOLDER_LEN_IDLE {
                    return Err(DecodeError::BadLength);
                }
                if bytes[1] != STATE_VERSION {
                    return Err(DecodeError::BadVersion);
                }
                let mut secret = [0_u8; 32];
                secret.copy_from_slice(&bytes[2..34]);
                match bytes[34] {
                    0 if bytes.len() == HOLDER_LEN_IDLE => Ok(Self::Holder { secret, pending: None }),
                    1 if bytes.len() == HOLDER_LEN_PENDING => {
                        let mut collection = [0_u8; 32];
                        collection.copy_from_slice(&bytes[35..67]);
                        let mut binding = [0_u8; 32];
                        binding.copy_from_slice(&bytes[67..99]);
                        Ok(Self::Holder {
                            secret,
                            pending: Some(Pending {
                                collection,
                                binding,
                                not_before: read_u64(&bytes[99..107]),
                                expires_at: read_u64(&bytes[107..115]),
                            }),
                        })
                    }
                    0 | 1 => Err(DecodeError::BadLength),
                    _ => Err(DecodeError::BadOption),
                }
            }
            Some(_) => Err(DecodeError::BadTag),
        }
    }
}

fn read_u64(bytes: &[u8]) -> u64 {
    let mut buf = [0_u8; 8];
    buf.copy_from_slice(bytes);
    u64::from_le_bytes(buf)
}

// ---------------------------------------------------------------------------------------------
// Outcome codes (spec §3.3)
// ---------------------------------------------------------------------------------------------

/// The nine outcome codes. Numbers are stable; they are what the wasm build returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Outcome {
    /// A current fact for this collection and context. Both verifiers.
    Ok = 0,
    /// The slot is empty. Both verifiers.
    NoFact = 1,
    /// The bytes are not a version-1 `Fact`. Both verifiers.
    Malformed = 2,
    /// The slot's fact is for another collection or context, or a newer proof replaced it.
    BindingMismatch = 3,
    /// Chain time is before `not_before`. Library; on LEZ the sequencer refuses.
    NotYetValid = 4,
    /// Chain time is at or past `expires_at`. Library; on LEZ the sequencer refuses.
    Expired = 5,
    /// `Act` was not signed by the acting account. Consumer.
    NotAuthorized = 6,
    /// Could not read the chain. Library.
    RpcUnavailable = 7,
    /// The gate refused in the wallet's simulation. Wallet only.
    NotAMember = 8,
}

impl Outcome {
    /// Every code, in number order.
    pub const ALL: [Self; 9] = [
        Self::Ok,
        Self::NoFact,
        Self::Malformed,
        Self::BindingMismatch,
        Self::NotYetValid,
        Self::Expired,
        Self::NotAuthorized,
        Self::RpcUnavailable,
        Self::NotAMember,
    ];

    /// The code as text, as the consumer panics with it and the libraries print it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Ok => "OK",
            Self::NoFact => "NO_FACT",
            Self::Malformed => "MALFORMED",
            Self::BindingMismatch => "BINDING_MISMATCH",
            Self::NotYetValid => "NOT_YET_VALID",
            Self::Expired => "EXPIRED",
            Self::NotAuthorized => "NOT_AUTHORIZED",
            Self::RpcUnavailable => "RPC_UNAVAILABLE",
            Self::NotAMember => "NOT_A_MEMBER",
        }
    }

    /// A plain sentence for a user.
    #[must_use]
    pub const fn plain(self) -> &'static str {
        match self {
            Self::Ok => "Valid: a current membership proof for this collection and this use.",
            Self::NoFact => "No proof: the pass id has no proof written to it.",
            Self::Malformed => "Not a proof: the data at this pass id is not a gate fact.",
            Self::BindingMismatch => {
                "Not for this use: the proof is for another collection or use, or a newer proof replaced it."
            }
            Self::NotYetValid => "Not valid yet: the proof's start time is still ahead.",
            Self::Expired => "Expired: the proof's validity window has ended.",
            Self::NotAuthorized => "Not authorised: the acting account did not sign.",
            Self::RpcUnavailable => "Could not read the chain: no answer from the RPC.",
            Self::NotAMember => "Not a member: the wallet found no NFT of this collection to prove with.",
        }
    }

    #[must_use]
    pub const fn code(self) -> u8 {
        self as u8
    }

    #[must_use]
    pub fn from_code(code: u8) -> Option<Self> {
        Self::ALL.get(usize::from(code)).copied()
    }

    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|o| o.name() == name)
    }
}

/// The one check (spec §3.1, §3.2): `state` is the slot's gate actor state as read,
/// `expected` the binding the verifier computed itself, `now_ms` the chain time the off-chain
/// library was told, or `None` on LEZ (a program cannot read the time; the sequencer enforces
/// the window).
///
/// Order, documented so every verifier reports the same code for the same bytes:
/// empty → `NO_FACT`; not a version-1 fact → `MALFORMED`; another binding →
/// `BINDING_MISMATCH`; then, with a time, before `not_before` → `NOT_YET_VALID`, at or past
/// `expires_at` → `EXPIRED`; else `OK`.
#[must_use]
pub fn check(state: &[u8], expected: &Bytes32, now_ms: Option<u64>) -> Outcome {
    check_detailed(state, expected, now_ms).0
}

/// [`check`], also returning the decoded fact whenever the bytes are one.
#[must_use]
pub fn check_detailed(state: &[u8], expected: &Bytes32, now_ms: Option<u64>) -> (Outcome, Option<Fact>) {
    if state.is_empty() {
        return (Outcome::NoFact, None);
    }
    let Ok(fact) = Fact::decode(state) else {
        return (Outcome::Malformed, None);
    };
    if !ct_eq(&fact.binding, expected) {
        return (Outcome::BindingMismatch, Some(fact));
    }
    if let Some(now) = now_ms {
        if now < fact.not_before {
            return (Outcome::NotYetValid, Some(fact));
        }
        if now >= fact.expires_at {
            return (Outcome::Expired, Some(fact));
        }
    }
    (Outcome::Ok, Some(fact))
}

fn ct_eq(a: &Bytes32, b: &Bytes32) -> bool {
    let mut diff = 0_u8;
    for i in 0..32 {
        diff |= a[i] ^ b[i];
    }
    diff == 0
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn fact_is_fifty_bytes() {
        assert_eq!(FACT_LEN, 50);
    }

    #[test]
    fn ttl_rule_picks_smallest_cover() {
        assert_eq!(ttl_class_for(1), Some(0));
        assert_eq!(ttl_class_for(3_600_000), Some(0));
        assert_eq!(ttl_class_for(3_600_001), Some(1));
        assert_eq!(ttl_class_for(86_400_000), Some(2));
        assert_eq!(ttl_class_for(86_400_001), None);
    }

    #[test]
    fn every_ttl_exceeds_the_grid() {
        // Spec §2.7: every TTL class must exceed QUANT_MS plus proving time.
        for ttl in TTL_MS {
            assert!(ttl > QUANT_MS);
        }
    }

    #[test]
    fn hex32_parses() {
        const X: Bytes32 = hex32("00ff10AB00000000000000000000000000000000000000000000000000000001");
        assert_eq!(X[0..4], [0x00, 0xff, 0x10, 0xab]);
        assert_eq!(X[31], 1);
        assert_eq!(deploy_constant(None, [9; 32]), [9; 32]);
    }

    #[test]
    fn codes_round_trip() {
        for o in Outcome::ALL {
            assert_eq!(Outcome::from_code(o.code()), Some(o));
            assert_eq!(Outcome::from_name(o.name()), Some(o));
        }
        assert_eq!(Outcome::from_code(9), None);
    }
}
