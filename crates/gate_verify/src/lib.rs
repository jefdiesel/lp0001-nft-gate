// SPDX-License-Identifier: MIT OR Apache-2.0
//! The app-code verifier (spec §3.2): check a membership proof without sending a transaction.
//!
//! Steps, exactly the spec's:
//! 1. Recompute the context from the app's own values. Never take it from the proof object.
//! 2. Compute the binding from the gate's pinned NFT program, the collection the app gates
//!    on, and that context.
//! 3. Fetch the gate actor state of `for_public_pda(gate, slot)` (`getAccountView`) and the
//!    latest block's timestamp (`getLastBlockId`, `getBlock`).
//! 4. `gate_core::check(state, binding, chain_timestamp)`.
//! 5. The caller marks its nonce used.
//!
//! **What this trusts.** The RPC, fully: for the slot state, the time, and that the gate
//! account runs the pinned immutable image. A lying RPC can show a fact that does not exist or
//! hide a newer one; a LEZ block header carries no state root (LEZ v0.4.0-rc3
//! `lez/common/src/block.rs:51-57`), so nothing here can check it. The sequencer: for having
//! verified the proof, the commitment root and the nullifiers when it included the
//! transaction, and for its clock. RISC Zero receipts, the LEZ privacy circuit, SHA-256.
//! Use your own node; several independent RPCs reduce the trust but do not remove it.

use gate_core::{Bytes32, Fact, Outcome, Sha2};
use serde_json::{json, Value};

/// What a verifier pins, from its own configuration (never from the proof object).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pinned {
    /// The gate program's account id.
    pub gate: Bytes32,
    /// The reference NFT program the gate is pinned to.
    pub nft_program: Bytes32,
    /// The collection (NFT definition id) the app gates on.
    pub collection: Bytes32,
    /// The zone's channel id (`getChannelId`).
    pub zone_id: Bytes32,
}

/// The proof object a wallet returns: `{ gate, slot, not_before, expires_at, tx_hash }`.
/// Only `slot` is used to find the fact; the chain's fact is what is checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofObject {
    pub gate: Bytes32,
    pub slot: Bytes32,
    pub not_before: u64,
    pub expires_at: u64,
    pub tx_hash: Option<Bytes32>,
}

/// The off-chain context's inputs, all the app's own (spec §2.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OffchainUse<'a> {
    /// The dApp identity as the wallet's provider sees the caller (the wallet fills the
    /// audience from it; the app must use the same string it is known by).
    pub caller_identity: &'a [u8],
    pub purpose: &'a [u8],
    /// The 32 random bytes the app issued for this session.
    pub nonce: Bytes32,
}

/// The on-chain context's inputs, as read from a `Gated` event or chosen before `Act`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OnchainUse {
    pub consumer: Bytes32,
    pub submission_id: Bytes32,
    pub actor: Bytes32,
    pub nonce: u64,
}

/// A verdict, with what was read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    pub outcome: Outcome,
    pub fact: Option<Fact>,
    /// The chain time the verdict was made at, when it was read.
    pub chain_time_ms: Option<u64>,
    /// The slot's account id.
    pub slot_account: Bytes32,
    /// The RPC error, for `RPC_UNAVAILABLE`.
    pub error: Option<String>,
}

/// What the verifier reads from the chain.
pub trait ChainReader {
    /// The actor state of `(account, program)`; empty when there is none.
    fn actor_state(&self, account: &Bytes32, program: &Bytes32) -> Result<Vec<u8>, String>;
    /// The latest block's timestamp, Unix milliseconds.
    fn latest_timestamp(&self) -> Result<u64, String>;
}

/// Step 1, off chain.
#[must_use]
pub fn offchain_context(pinned: &Pinned, u: &OffchainUse<'_>) -> Bytes32 {
    gate_core::context_offchain::<Sha2>(&pinned.zone_id, &pinned.gate, u.caller_identity, u.purpose, &u.nonce)
}

/// Step 1, on chain (to re-check an `Act` fact in app code, spec §5 "same proof, both ways").
#[must_use]
pub fn onchain_context(pinned: &Pinned, u: &OnchainUse) -> Bytes32 {
    gate_core::context_onchain::<Sha2>(&pinned.zone_id, &pinned.gate, &u.consumer, &u.submission_id, &u.actor, u.nonce)
}

/// Steps 2-4 for a context the app computed itself.
pub fn verify_context(reader: &impl ChainReader, pinned: &Pinned, slot: &Bytes32, context: &Bytes32) -> Verdict {
    let expected = gate_core::binding::<Sha2>(&pinned.nft_program, &pinned.collection, context);
    let pda = gate_core::slot_pda::<Sha2>(&pinned.gate, slot);
    let unavailable = |error: String| Verdict {
        outcome: Outcome::RpcUnavailable,
        fact: None,
        chain_time_ms: None,
        slot_account: pda,
        error: Some(error),
    };
    let state = match reader.actor_state(&pda, &pinned.gate) {
        Ok(s) => s,
        Err(e) => return unavailable(e),
    };
    let now = match reader.latest_timestamp() {
        Ok(t) => t,
        Err(e) => return unavailable(e),
    };
    let (outcome, fact) = gate_core::check_detailed(&state, &expected, Some(now));
    Verdict { outcome, fact, chain_time_ms: Some(now), slot_account: pda, error: None }
}

/// The whole off-chain check of a proof object. A proof object for another gate than the
/// pinned one is not looked at: `BINDING_MISMATCH` (the binding names the pinned gate).
pub fn verify_offchain(reader: &impl ChainReader, pinned: &Pinned, proof: &ProofObject, u: &OffchainUse<'_>) -> Verdict {
    if proof.gate != pinned.gate {
        return Verdict {
            outcome: Outcome::BindingMismatch,
            fact: None,
            chain_time_ms: None,
            slot_account: gate_core::slot_pda::<Sha2>(&pinned.gate, &proof.slot),
            error: Some("the proof names another gate than the pinned one".to_owned()),
        };
    }
    verify_context(reader, pinned, &proof.slot, &offchain_context(pinned, u))
}

// ---------------------------------------------------------------------------------------------
// JSON-RPC shapes (LEZ v0.4.0-rc3 `lez/sequencer/service/rpc/src/lib.rs`)
// ---------------------------------------------------------------------------------------------

/// A JSON-RPC 2.0 transport: `method` with positional `params`, returning `result`.
pub trait RpcJson {
    fn call(&self, method: &str, params: Value) -> Result<Value, String>;
}

/// [`ChainReader`] over any [`RpcJson`] transport.
pub struct JsonRpcReader<T: RpcJson>(pub T);

impl<T: RpcJson> ChainReader for JsonRpcReader<T> {
    fn actor_state(&self, account: &Bytes32, program: &Bytes32) -> Result<Vec<u8>, String> {
        let params = json!([{ "account_id": b58encode(account), "program_account_id": b58encode(program) }]);
        let result = self.0.call("getAccountView", params)?;
        actor_state_from_account_json(&result, program)
    }

    fn latest_timestamp(&self) -> Result<u64, String> {
        let id = self.0.call("getLastBlockId", json!([]))?;
        let id = id.as_u64().ok_or("getLastBlockId: not a number")?;
        let block = self.0.call("getBlock", json!([id]))?;
        if block.is_null() {
            return Err(format!("getBlock({id}): no block"));
        }
        let b64 = block.as_str().ok_or("getBlock: not a base64 string")?;
        block_timestamp_from_borsh(&b64decode(b64)?)
    }
}

/// `getAccountView` result → the actor state at `program`. `null` (no account) is empty.
/// Shape: `{"nonce":N,"data":{"actor_states":{"<base58>":[bytes...]}}}` (lee_core
/// `account.rs` serde test at :492-498).
pub fn actor_state_from_account_json(result: &Value, program: &Bytes32) -> Result<Vec<u8>, String> {
    if result.is_null() {
        return Ok(Vec::new());
    }
    let states = result
        .get("data")
        .and_then(|d| d.get("actor_states"))
        .and_then(Value::as_object)
        .ok_or("getAccountView: no data.actor_states")?;
    let Some(bytes) = states.get(&b58encode(program)) else {
        return Ok(Vec::new());
    };
    bytes
        .as_array()
        .ok_or("getAccountView: actor state is not a byte array")?
        .iter()
        .map(|b| b.as_u64().and_then(|v| u8::try_from(v).ok()).ok_or_else(|| "getAccountView: not a byte".to_owned()))
        .collect()
}

/// A `Block` is JSON-encoded as base64 of its borsh bytes (LEZ `lez/common/src/block.rs`,
/// `impl Serialize for Block`). Its header comes first: `block_id u64`, `prev_block_hash`
/// 32 bytes, `hash` 32 bytes, `timestamp u64` (`block.rs:51-57`): the timestamp is bytes
/// 72..80, little-endian.
pub fn block_timestamp_from_borsh(bytes: &[u8]) -> Result<u64, String> {
    let ts = bytes.get(72..80).ok_or("getBlock: block shorter than its header")?;
    let mut buf = [0_u8; 8];
    buf.copy_from_slice(ts);
    Ok(u64::from_le_bytes(buf))
}

const B58: &[u8; 58] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

/// Base58 (Bitcoin alphabet), as LEZ prints account ids.
#[must_use]
pub fn b58encode(bytes: &[u8]) -> String {
    let zeros = bytes.iter().take_while(|&&b| b == 0).count();
    let mut digits: Vec<u8> = Vec::new();
    for &byte in bytes {
        let mut carry = u32::from(byte);
        for d in &mut digits {
            carry += u32::from(*d) << 8;
            *d = u8::try_from(carry % 58).unwrap_or(0);
            carry /= 58;
        }
        while carry > 0 {
            digits.push(u8::try_from(carry % 58).unwrap_or(0));
            carry /= 58;
        }
    }
    let mut out = String::with_capacity(zeros + digits.len());
    out.extend(std::iter::repeat('1').take(zeros));
    out.extend(digits.iter().rev().map(|&d| char::from(B58[usize::from(d)])));
    out
}

/// Base58 to 32 bytes (an account id).
pub fn b58decode32(text: &str) -> Result<Bytes32, String> {
    let mut bytes = [0_u8; 32];
    for c in text.trim().chars() {
        let digit = B58
            .iter()
            .position(|&a| char::from(a) == c)
            .ok_or_else(|| format!("bad base58 character {c:?}"))?;
        let mut carry = u32::try_from(digit).unwrap_or(0);
        for slot in bytes.iter_mut().rev() {
            let v = u32::from(*slot) * 58 + carry;
            *slot = u8::try_from(v & 0xff).unwrap_or(0);
            carry = v >> 8;
        }
        if carry != 0 {
            return Err(format!("'{text}' is longer than 32 bytes"));
        }
    }
    Ok(bytes)
}

/// Standard base64 with padding.
pub fn b64decode(text: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut acc = 0_u32;
    let mut bits = 0_u32;
    for c in text.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            b'\n' | b'\r' => continue,
            _ => return Err(format!("bad base64 character {:?}", char::from(c))),
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(u8::try_from((acc >> bits) & 0xff).unwrap_or(0));
        }
    }
    Ok(out)
}

/// Lower-case hex.
#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// 64 hex characters to 32 bytes.
pub fn unhex32(text: &str) -> Result<Bytes32, String> {
    let t = text.trim().trim_start_matches("0x");
    if t.len() != 64 {
        return Err(format!("expected 64 hex characters, got {}", t.len()));
    }
    let mut out = [0_u8; 32];
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&t[i * 2..i * 2 + 2], 16).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
