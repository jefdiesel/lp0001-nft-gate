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
//! account runs the pinned immutable image. `verify_onchain_use` also trusts the LEZ
//! indexer for the Act transaction's `Gated` event (actor and nonce); it checks the event's
//! emitter, selector and submission id, but a lying indexer can still substitute an event. A lying RPC can show a fact that does not exist or
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

/// The on-chain use's inputs. The acting account and nonce are read from the Act
/// transaction's `Gated` event, not taken from the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OnchainUse {
    /// The consumer program that must have emitted the event.
    pub consumer: Bytes32,
    pub submission_id: Bytes32,
    /// The hash of the `Act` transaction whose event is read.
    pub tx_hash: Bytes32,
}

/// One record from the LEZ indexer's `getEvents` reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventRecord {
    /// The emitting *program* (LEZ sets `TransactionEvent.account_id` to the program).
    pub program_account_id: Bytes32,
    pub selector: [u8; 8],
    pub data: Vec<u8>,
    pub block_id: Option<u64>,
}

/// The fields read from a borsh-encoded `Gated` event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GatedFields {
    pub slot: Bytes32,
    pub actor: Bytes32,
    pub nonce: u64,
    pub expires_at: u64,
}

/// The Gated event selector: sha256("lp0001-consumer::Gated")[..8].
#[must_use]
pub fn gated_selector() -> [u8; 8] {
    let digest = <Sha2 as gate_core::Sha256>::hash(&[b"lp0001-consumer::Gated"]);
    let mut out = [0_u8; 8];
    out.copy_from_slice(&digest[..8]);
    out
}

/// Decode a Gated event's borsh data: submission id, slot, actor (32 bytes each),
/// then nonce and expires_at as little-endian u64 (gate_interface's `Gated`).
/// Returns None unless the submission id is the one this consumer expects.
#[must_use]
pub fn decode_gated_event(data: &[u8], submission_id: &Bytes32) -> Option<GatedFields> {
    if data.len() < 112 || &data[..32] != submission_id {
        return None;
    }
    let mut slot = [0_u8; 32];
    slot.copy_from_slice(&data[32..64]);
    let mut actor = [0_u8; 32];
    actor.copy_from_slice(&data[64..96]);
    Some(GatedFields {
        slot,
        actor,
        nonce: u64::from_le_bytes(data[96..104].try_into().ok()?),
        expires_at: u64::from_le_bytes(data[104..112].try_into().ok()?),
    })
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
    /// The Gated events of one transaction hash, from the LEZ indexer RPC.
    fn gated_events(&self, tx_hash: &Bytes32, consumer: &Bytes32) -> Result<Vec<EventRecord>, String>;
}

/// Step 1, off chain.
#[must_use]
pub fn offchain_context(pinned: &Pinned, u: &OffchainUse<'_>) -> Bytes32 {
    gate_core::context_offchain::<Sha2>(&pinned.zone_id, &pinned.gate, u.caller_identity, u.purpose, &u.nonce)
}

/// Step 1, on chain: the context a consumer rebuilt at `actor`/`nonce` (spec §3.1).
#[must_use]
pub fn onchain_context(
    pinned: &Pinned,
    consumer: &Bytes32,
    submission_id: &Bytes32,
    actor: &Bytes32,
    nonce: u64,
) -> Bytes32 {
    gate_core::context_onchain::<Sha2>(&pinned.zone_id, &pinned.gate, consumer, submission_id, actor, nonce)
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

/// Re-check an on-chain use in app code, without a transaction (spec §5, "same proof,
/// both ways"). The acting account and nonce come from the Act transaction's Gated
/// event over the LEZ indexer RPC, never from the caller. A record is accepted only
/// if its emitter is the pinned consumer, its selector is Gated's, and its embedded
/// submission id is the one expected. Refusals reuse the same codes as the JS library:
/// NO_FACT (no such event), BINDING_MISMATCH (another pass), RPC_UNAVAILABLE.
#[must_use]
pub fn verify_onchain_use(
    reader: &impl ChainReader,
    pinned: &Pinned,
    proof: &ProofObject,
    u: &OnchainUse,
) -> Verdict {
    let pda = gate_core::slot_pda::<Sha2>(&pinned.gate, &proof.slot);
    let refusal = |outcome: Outcome, error: &str| Verdict {
        outcome,
        fact: None,
        chain_time_ms: None,
        slot_account: pda,
        error: Some(error.to_owned()),
    };
    let events = match reader.gated_events(&u.tx_hash, &u.consumer) {
        Ok(events) => events,
        Err(error) => return refusal(Outcome::RpcUnavailable, &error),
    };
    for event in events {
        // Any program can emit the selector; the RPC filter is only an optimisation.
        if event.program_account_id != u.consumer || event.selector != gated_selector() {
            continue;
        }
        let Some(gated) = decode_gated_event(&event.data, &u.submission_id) else {
            continue;
        };
        if gated.slot != proof.slot {
            return refusal(Outcome::BindingMismatch, "the Gated event names a different pass");
        }
        let context = onchain_context(pinned, &u.consumer, &u.submission_id, &gated.actor, gated.nonce);
        return verify_context(reader, pinned, &proof.slot, &context);
    }
    refusal(Outcome::NoFact, "no Gated event from this consumer for this transaction and submission id")
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

    fn gated_events(&self, tx_hash: &Bytes32, consumer: &Bytes32) -> Result<Vec<EventRecord>, String> {
        // LEZ indexer RPC `getEvents` with a tx-hash point query (LEZ rc3
        // `lez/indexer/service/rpc/src/lib.rs:113-116`, filter at
        // `lez/indexer/service/protocol/src/lib.rs:701-707`). Account ids are base58,
        // hashes and selectors hex, event data base64.
        let params = json!([{
            "tx_hash": hex(tx_hash),
            "program_account_id": b58encode(consumer),
            "selector": hex(&gated_selector()),
        }]);
        let result = self.0.call("getEvents", params)?;
        let rows = result.as_array().ok_or("getEvents: not an array")?;
        let mut out = Vec::new();
        for row in rows {
            let program_text = row
                .get("program_account_id")
                .and_then(Value::as_str)
                .ok_or("getEvents: no program_account_id")?;
            let selector_text = row.get("selector").and_then(Value::as_str).ok_or("getEvents: no selector")?;
            let data_text = row.get("data").and_then(Value::as_str).ok_or("getEvents: no data")?;
            let mut selector = [0_u8; 8];
            let bytes = unhex(selector_text)?;
            if bytes.len() != 8 {
                return Err("getEvents: selector is not 8 bytes".to_owned());
            }
            selector.copy_from_slice(&bytes);
            out.push(EventRecord {
                program_account_id: b58decode32(program_text)?,
                selector,
                data: b64decode(data_text)?,
                block_id: row.get("block_id").and_then(Value::as_u64),
            });
        }
        Ok(out)
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

/// Hex to bytes (any even length).
fn unhex(text: &str) -> Result<Vec<u8>, String> {
    let t = text.trim().trim_start_matches("0x");
    if t.len() % 2 != 0 {
        return Err("odd-length hex".to_owned());
    }
    (0..t.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&t[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect()
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
