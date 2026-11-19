// SPDX-License-Identifier: MIT OR Apache-2.0
//! `gate_core` for `wasm32-unknown-unknown`, with a plain C ABI and no wasm-bindgen.
//!
//! The JS side (`js/gate-verify/src/wasm.mjs`) copies inputs into the module's memory at
//! offsets it gets from [`gate_buf`], calls a function, and reads 32-byte results back from
//! the same buffer. Every function returns 0 on success or the outcome code.
//!
//! Status: WRITTEN, NOT BUILT. The wasm32 target is not installed on the build drive; the
//! build command is in RUN-AFTER-3AM.txt. Native tests below run the same functions.
// No `unsafe` blocks; `#[no_mangle]` (an unsafe attribute) exports the C ABI.

use gate_core::{Bytes32, Sha2};
use std::sync::Mutex;

/// One shared I/O buffer: inputs at the front, the 32-byte result at `OUT`.
const BUF_LEN: usize = 4096;
const OUT: usize = BUF_LEN - 32;
static BUF: Mutex<[u8; BUF_LEN]> = Mutex::new([0; BUF_LEN]);

fn with_buf<R>(f: impl FnOnce(&mut [u8; BUF_LEN]) -> R) -> R {
    let mut guard = BUF.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    f(&mut guard)
}

fn take32(buf: &[u8], at: usize) -> Option<Bytes32> {
    buf.get(at..at + 32)?.try_into().ok()
}

/// The buffer's address in linear memory, for the JS side to write into and read from.
#[no_mangle]
pub extern "C" fn gate_buf() -> usize {
    with_buf(|b| b.as_ptr() as usize)
}

/// The buffer's length.
#[no_mangle]
pub extern "C" fn gate_buf_len() -> usize {
    BUF_LEN
}

/// `check(state, expected, now)`. Input layout: `[0..32)` expected binding, then the state
/// bytes (`state_len`). `has_now` 0 means no time (the on-chain form). Returns the code.
#[no_mangle]
pub extern "C" fn gate_check(state_len: usize, has_now: u32, now_ms: u64) -> u32 {
    with_buf(|b| {
        let Some(expected) = take32(b, 0) else { return u32::from(gate_core::Outcome::Malformed.code()) };
        let Some(state) = b.get(32..32 + state_len) else { return u32::from(gate_core::Outcome::Malformed.code()) };
        let now = (has_now != 0).then_some(now_ms);
        u32::from(gate_core::check(state, &expected, now).code())
    })
}

/// `binding(nft, collection, context)`. Input: three 32-byte values at 0, 32, 64.
#[no_mangle]
pub extern "C" fn gate_binding() -> u32 {
    with_buf(|b| {
        let (Some(n), Some(c), Some(x)) = (take32(b, 0), take32(b, 32), take32(b, 64)) else { return 1 };
        b[OUT..].copy_from_slice(&gate_core::binding::<Sha2>(&n, &c, &x));
        0
    })
}

/// `context_offchain(zone, gate, caller_identity, purpose, nonce)`. Input: zone 0, gate 32,
/// nonce 64, then `id_len` bytes of caller identity, then `purpose_len` bytes of purpose.
#[no_mangle]
pub extern "C" fn gate_context_offchain(id_len: usize, purpose_len: usize) -> u32 {
    with_buf(|b| {
        let (Some(z), Some(g), Some(n)) = (take32(b, 0), take32(b, 32), take32(b, 64)) else { return 1 };
        let Some(id) = b.get(96..96 + id_len).map(<[u8]>::to_vec) else { return 1 };
        let Some(purpose) = b.get(96 + id_len..96 + id_len + purpose_len).map(<[u8]>::to_vec) else { return 1 };
        if 96 + id_len + purpose_len > OUT {
            return 1;
        }
        b[OUT..].copy_from_slice(&gate_core::context_offchain::<Sha2>(&z, &g, &id, &purpose, &n));
        0
    })
}

/// `context_onchain(zone, gate, consumer, submission_id, actor, nonce)`. Input: five 32-byte
/// values at 0, 32, 64, 96, 128.
#[no_mangle]
pub extern "C" fn gate_context_onchain(nonce: u64) -> u32 {
    with_buf(|b| {
        let vals: Option<Vec<Bytes32>> = (0..5).map(|i| take32(b, i * 32)).collect();
        let Some(v) = vals else { return 1 };
        b[OUT..].copy_from_slice(&gate_core::context_onchain::<Sha2>(&v[0], &v[1], &v[2], &v[3], &v[4], nonce));
        0
    })
}

/// `slot_pda(gate, slot)`. Input: gate 0, slot 32.
#[no_mangle]
pub extern "C" fn gate_slot_pda() -> u32 {
    with_buf(|b| {
        let (Some(g), Some(s)) = (take32(b, 0), take32(b, 32)) else { return 1 };
        b[OUT..].copy_from_slice(&gate_core::slot_pda::<Sha2>(&g, &s));
        0
    })
}

/// Reads the 32-byte result (native tests).
pub fn result() -> Bytes32 {
    with_buf(|b| take32(b, OUT).unwrap_or([0; 32]))
}

/// Writes input bytes at `at` (native tests; the JS side writes memory directly).
pub fn write(at: usize, data: &[u8]) {
    with_buf(|b| b[at..at + data.len()].copy_from_slice(data));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_abi_computes_what_gate_core_computes() {
        let (n, c, x) = ([1; 32], [2; 32], [3; 32]);
        write(0, &n);
        write(32, &c);
        write(64, &x);
        assert_eq!(gate_binding(), 0);
        let b = result();
        assert_eq!(b, gate_core::binding::<Sha2>(&n, &c, &x));
        let fact = gate_core::Fact { binding: b, not_before: 600_000, expires_at: 600_000 + 3_600_000 }.encode();
        write(0, &b);
        write(32, &fact);
        assert_eq!(gate_check(50, 1, 600_001), 0);
        assert_eq!(gate_check(50, 1, 600_000 + 3_600_000), 5);
        assert_eq!(gate_check(0, 0, 0), 1);
        assert_eq!(gate_check(49, 0, 0), 2);
        write(0, &[9; 32]);
        assert_eq!(gate_check(50, 0, 0), 3);
    }
}
