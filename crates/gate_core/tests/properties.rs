// SPDX-License-Identifier: MIT OR Apache-2.0
//! Property tests for every rule of `check()`, over many random inputs.
//!
//! A small deterministic generator (xorshift64*) instead of a property-testing crate, so the
//! test needs nothing beyond `sha2`. Seeds are fixed: a failure names its iteration.

use gate_core::*;

struct S;
impl Sha256 for S {
    fn hash(parts: &[&[u8]]) -> Bytes32 {
        use sha2::Digest as _;
        let mut h = sha2::Sha256::new();
        for p in parts {
            h.update(p);
        }
        h.finalize().into()
    }
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    fn b32(&mut self) -> Bytes32 {
        let mut out = [0_u8; 32];
        for chunk in out.chunks_mut(8) {
            chunk.copy_from_slice(&self.next().to_le_bytes());
        }
        out
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    /// A not_before on the grid, somewhere in 2020-2100.
    fn grid_time(&mut self) -> u64 {
        let start = 1_577_836_800_000 / QUANT_MS;
        (start + self.below(4_000_000)) * QUANT_MS
    }
}

const N: u64 = 2_000;

/// A random but well-formed world: an NFT program, a collection, a context, a fact.
struct World {
    nft: Bytes32,
    collection: Bytes32,
    context: Bytes32,
    fact: Fact,
}

fn world(r: &mut Rng) -> World {
    let nft = r.b32();
    let collection = r.b32();
    let zone = r.b32();
    let gate = r.b32();
    let consumer = r.b32();
    let sub = r.b32();
    let actor = r.b32();
    let nonce = r.next();
    let context = context_onchain::<S>(&zone, &gate, &consumer, &sub, &actor, nonce);
    let nb = r.grid_time();
    let ttl = TTL_MS[usize::try_from(r.below(3)).unwrap()];
    let fact = Fact { binding: binding::<S>(&nft, &collection, &context), not_before: nb, expires_at: nb + ttl };
    World { nft, collection, context, fact }
}

fn inside(r: &mut Rng, f: &Fact) -> u64 {
    f.not_before + r.below(f.expires_at - f.not_before)
}

#[test]
fn a_right_fact_inside_its_window_is_ok() {
    let mut r = Rng(1);
    for i in 0..N {
        let w = world(&mut r);
        let now = inside(&mut r, &w.fact);
        let b = binding::<S>(&w.nft, &w.collection, &w.context);
        assert_eq!(check(&w.fact.encode(), &b, Some(now)), Outcome::Ok, "iteration {i}");
        assert_eq!(check(&w.fact.encode(), &b, None), Outcome::Ok, "iteration {i} on chain");
    }
}

#[test]
fn wrong_collection_is_binding_mismatch() {
    let mut r = Rng(2);
    for i in 0..N {
        let w = world(&mut r);
        let other = r.b32();
        assert_ne!(other, w.collection);
        let b = binding::<S>(&w.nft, &other, &w.context);
        let now = inside(&mut r, &w.fact);
        assert_eq!(check(&w.fact.encode(), &b, Some(now)), Outcome::BindingMismatch, "iteration {i}");
        assert_eq!(check(&w.fact.encode(), &b, None), Outcome::BindingMismatch, "iteration {i}");
    }
}

#[test]
fn wrong_context_is_binding_mismatch() {
    let mut r = Rng(3);
    for i in 0..N {
        let w = world(&mut r);
        // Change one field of the context at random: every field is bound.
        let field = r.below(6);
        let mut parts = [r.b32(), r.b32(), r.b32(), r.b32(), r.b32()];
        let nonce = r.next();
        let base = context_onchain::<S>(&parts[0], &parts[1], &parts[2], &parts[3], &parts[4], nonce);
        let fact = Fact { binding: binding::<S>(&w.nft, &w.collection, &base), ..w.fact };
        let mut n2 = nonce;
        if field < 5 {
            parts[usize::try_from(field).unwrap()][usize::try_from(r.below(32)).unwrap()] ^= 1 << r.below(8);
        } else {
            n2 = nonce.wrapping_add(1 + r.below(1000));
        }
        let other = context_onchain::<S>(&parts[0], &parts[1], &parts[2], &parts[3], &parts[4], n2);
        assert_ne!(other, base);
        let b = binding::<S>(&w.nft, &w.collection, &other);
        assert_eq!(check(&fact.encode(), &b, None), Outcome::BindingMismatch, "iteration {i} field {field}");
    }
}

#[test]
fn onchain_and_offchain_contexts_never_meet() {
    let mut r = Rng(4);
    for i in 0..N {
        let (zone, gate, aud, payload) = (r.b32(), r.b32(), r.b32(), r.b32());
        let on = context::<S>(KIND_ONCHAIN, &zone, &gate, &aud, &payload);
        let off = context::<S>(KIND_OFFCHAIN, &zone, &gate, &aud, &payload);
        assert_ne!(on, off, "iteration {i}");
    }
}

#[test]
fn a_look_alike_nft_program_is_binding_mismatch() {
    let mut r = Rng(5);
    for i in 0..N {
        let w = world(&mut r);
        let fake = r.b32();
        let b = binding::<S>(&fake, &w.collection, &w.context);
        assert_eq!(check(&w.fact.encode(), &b, None), Outcome::BindingMismatch, "iteration {i}");
    }
}

#[test]
fn superseded_fact_is_binding_mismatch_for_the_old_context() {
    // One slot holds one fact: a second proof for context Y overwrites X's fact.
    let mut r = Rng(6);
    for i in 0..N {
        let w = world(&mut r);
        let y = r.b32();
        let newer = Fact { binding: binding::<S>(&w.nft, &w.collection, &y), ..w.fact };
        let old_b = binding::<S>(&w.nft, &w.collection, &w.context);
        let now = inside(&mut r, &newer);
        assert_eq!(check(&newer.encode(), &old_b, Some(now)), Outcome::BindingMismatch, "iteration {i}");
        assert_eq!(check(&newer.encode(), &newer.binding, Some(now)), Outcome::Ok, "iteration {i}");
    }
}

#[test]
fn expired_at_or_after_the_end() {
    let mut r = Rng(7);
    for i in 0..N {
        let w = world(&mut r);
        let now = w.fact.expires_at + r.below(10_000_000_000);
        assert_eq!(check(&w.fact.encode(), &w.fact.binding, Some(now)), Outcome::Expired, "iteration {i}");
        // On chain there is no clock: the sequencer, not check(), refuses late.
        assert_eq!(check(&w.fact.encode(), &w.fact.binding, None), Outcome::Ok, "iteration {i}");
    }
}

#[test]
fn not_yet_valid_before_the_start() {
    let mut r = Rng(8);
    for i in 0..N {
        let w = world(&mut r);
        let now = w.fact.not_before - 1 - r.below(w.fact.not_before);
        assert_eq!(check(&w.fact.encode(), &w.fact.binding, Some(now)), Outcome::NotYetValid, "iteration {i}");
    }
}

#[test]
fn binding_is_checked_before_time() {
    let mut r = Rng(9);
    for i in 0..N {
        let w = world(&mut r);
        let wrong = r.b32();
        let now = if r.below(2) == 0 { w.fact.expires_at + r.below(1 << 40) } else { r.below(w.fact.not_before) };
        assert_eq!(check(&w.fact.encode(), &wrong, Some(now)), Outcome::BindingMismatch, "iteration {i}");
    }
}

#[test]
fn empty_is_no_fact() {
    let mut r = Rng(10);
    for _ in 0..100 {
        let b = r.b32();
        assert_eq!(check(&[], &b, Some(r.next())), Outcome::NoFact);
        assert_eq!(check(&[], &b, None), Outcome::NoFact);
    }
}

#[test]
fn random_bytes_are_malformed_never_ok() {
    let mut r = Rng(11);
    for i in 0..N * 5 {
        let len = usize::try_from(1 + r.below(120)).unwrap();
        let bytes: Vec<u8> = (0..len).map(|_| u8::try_from(r.below(256)).unwrap()).collect();
        let b = r.b32();
        let got = check(&bytes, &b, Some(r.next()));
        // A 50-byte random string can parse as a fact only with a valid tag, version and a
        // gate-shaped window: astronomically unlikely, and then its binding still mismatches.
        assert!(
            got == Outcome::Malformed || got == Outcome::BindingMismatch,
            "iteration {i}: {got:?}"
        );
    }
}

#[test]
fn every_mutation_of_the_header_or_length_is_malformed() {
    let mut r = Rng(12);
    for i in 0..N {
        let w = world(&mut r);
        let good = w.fact.encode();
        // Truncate or extend.
        let cut = usize::try_from(r.below(50)).unwrap();
        assert_eq!(check(&good[..cut.max(1)], &w.fact.binding, None), Outcome::Malformed, "iteration {i} cut {cut}");
        let mut longer = good.to_vec();
        longer.push(u8::try_from(r.below(256)).unwrap());
        assert_eq!(check(&longer, &w.fact.binding, None), Outcome::Malformed, "iteration {i} longer");
        // Tag or version.
        let mut bad = good;
        let which = usize::try_from(r.below(2)).unwrap();
        bad[which] = bad[which].wrapping_add(1 + u8::try_from(r.below(255)).unwrap());
        assert_eq!(check(&bad, &w.fact.binding, None), Outcome::Malformed, "iteration {i} header {which}");
    }
}

#[test]
fn times_the_gate_cannot_write_are_malformed() {
    let mut r = Rng(13);
    for i in 0..N {
        let w = world(&mut r);
        let mut f = w.fact;
        match r.below(3) {
            0 => {
                // off the grid
                let shift = 1 + r.below(QUANT_MS - 1);
                f.not_before += shift;
                f.expires_at += shift;
            }
            1 => {
                // a TTL not on the menu
                let mut ttl = 1 + r.below(MAX_TTL_MS * 2);
                while TTL_MS.contains(&ttl) {
                    ttl += 1;
                }
                f.expires_at = f.not_before + ttl;
            }
            _ => {
                // reversed
                core::mem::swap(&mut f.not_before, &mut f.expires_at);
            }
        }
        assert_eq!(check(&f.encode(), &f.binding, Some(f.not_before)), Outcome::Malformed, "iteration {i}");
    }
}

#[test]
fn holder_states_round_trip_and_are_never_facts() {
    let mut r = Rng(14);
    for i in 0..N {
        let secret = r.b32();
        let pending = if r.below(2) == 0 {
            None
        } else {
            Some(Pending { collection: r.b32(), binding: r.b32(), not_before: r.next(), expires_at: r.next() })
        };
        let s = GateState::Holder { secret, pending };
        let mut buf = [0_u8; HOLDER_LEN_PENDING];
        let len = s.encode_into(&mut buf);
        assert_eq!(GateState::decode(&buf[..len]), Ok(s), "iteration {i}");
        assert_eq!(check(&buf[..len], &r.b32(), None), Outcome::Malformed, "iteration {i}");
    }
}

#[test]
fn window_rule_matches_what_check_accepts() {
    let mut r = Rng(15);
    for i in 0..N {
        let nb = if r.below(4) == 0 { r.next() } else { r.grid_time() };
        let class = u8::try_from(r.below(5)).unwrap();
        match window(nb, class) {
            Some((a, b)) => {
                assert!(nb % QUANT_MS == 0 && class < 3, "iteration {i}");
                let f = Fact { binding: [7; 32], not_before: a, expires_at: b };
                assert_eq!(check(&f.encode(), &f.binding, Some(a)), Outcome::Ok, "iteration {i}");
            }
            None => assert!(nb % QUANT_MS != 0 || class >= 3 || nb.checked_add(MAX_TTL_MS).is_none(), "iteration {i}"),
        }
    }
}

#[test]
fn wallet_time_rule() {
    let mut r = Rng(16);
    for i in 0..N {
        let now = r.next() >> 2;
        let nb = not_before_for(now);
        assert!(nb <= now && now - nb < QUANT_MS && nb % QUANT_MS == 0, "iteration {i}");
    }
}

#[test]
fn slots_differ_per_secret_and_pdas_per_gate() {
    let mut r = Rng(17);
    for i in 0..N {
        let (s1, s2, g1, g2) = (r.b32(), r.b32(), r.b32(), r.b32());
        assert_ne!(slot::<S>(&s1), slot::<S>(&s2), "iteration {i}");
        let sl = slot::<S>(&s1);
        assert_ne!(slot_pda::<S>(&g1, &sl), slot_pda::<S>(&g2, &sl), "iteration {i}");
    }
}
