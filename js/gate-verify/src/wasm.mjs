// SPDX-License-Identifier: MIT OR Apache-2.0
// Loader for the wasm build of gate_core (crates/gate_wasm), the same Rust check() the guests
// run. STATUS: written, not run — the wasm32 target is not on the build drive yet; the build
// command is in RUN-AFTER-3AM.txt. When the .wasm exists, test/wasm.test.mjs compares it with
// the JS implementation over every vector.

const OUT_FROM_END = 32;

/** Instantiate from bytes (node: fs.readFileSync; browser: fetch(...).arrayBuffer()). */
export async function loadGateWasm(bytes) {
  const { instance } = await WebAssembly.instantiate(bytes, {});
  const x = instance.exports;
  const mem = () => new Uint8Array(x.memory.buffer);
  const base = () => Number(x.gate_buf());
  const len = Number(x.gate_buf_len());
  const write = (at, data) => mem().set(data, base() + at);
  const result = () => mem().slice(base() + len - OUT_FROM_END, base() + len);
  return {
    check(state, expected, nowMs = null) {
      if (32 + state.length > len - OUT_FROM_END) throw new Error('state too large');
      write(0, expected);
      write(32, state);
      const has = nowMs === null || nowMs === undefined ? 0 : 1;
      return x.gate_check(state.length, has, BigInt(has ? nowMs : 0));
    },
    binding(nft, collection, ctx) {
      write(0, nft); write(32, collection); write(64, ctx);
      if (x.gate_binding() !== 0) throw new Error('gate_binding failed');
      return result();
    },
    contextOffchain(zone, gate, callerIdentity, purpose, nonce) {
      write(0, zone); write(32, gate); write(64, nonce);
      write(96, callerIdentity); write(96 + callerIdentity.length, purpose);
      if (x.gate_context_offchain(callerIdentity.length, purpose.length) !== 0) throw new Error('gate_context_offchain failed');
      return result();
    },
    contextOnchain(zone, gate, consumer, submissionId, actor, nonce) {
      [zone, gate, consumer, submissionId, actor].forEach((v, i) => write(i * 32, v));
      if (x.gate_context_onchain(BigInt(nonce)) !== 0) throw new Error('gate_context_onchain failed');
      return result();
    },
    slotPda(gate, slot) {
      write(0, gate); write(32, slot);
      if (x.gate_slot_pda() !== 0) throw new Error('gate_slot_pda failed');
      return result();
    },
  };
}
