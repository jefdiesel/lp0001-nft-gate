// SPDX-License-Identifier: MIT OR Apache-2.0
//! A different zone id must produce a different context, binding and verdict
//! (spec §2.4, §7). The golden vectors fix one zone; this test varies it.

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

fn inputs() -> (Bytes32, Bytes32, Bytes32, Bytes32, Bytes32, u64, Bytes32) {
    // NFT program, collection, gate, audience, payload, acting nonce, acting account.
    (
        [0xAA; 32],
        [0xC0; 32],
        [0x6A; 32],
        [0xCC; 32],
        [0x5B; 32],
        7,
        [0xA1; 32],
    )
}

#[test]
fn a_different_zone_gives_a_different_binding_and_the_old_fact_reads_binding_mismatch() {
    let (nft, collection, gate, audience, submission, nonce, actor) = inputs();
    let nb = 1_763_500_200_000; // on the QUANT_MS grid
    let expires = nb + TTL_MS[0];
    let zone_a = [0x2E; 32];
    let zone_b = [0x2F; 32];

    // On-chain context: the consumer's audience and the acting account/nonce.
    let ctx_on_a = context_onchain::<S>(&zone_a, &gate, &audience, &submission, &actor, nonce);
    let ctx_on_b = context_onchain::<S>(&zone_b, &gate, &audience, &submission, &actor, nonce);
    assert_ne!(ctx_on_a, ctx_on_b, "the zone is in the on-chain context");
    let bind_on_a = binding::<S>(&nft, &collection, &ctx_on_a);
    let bind_on_b = binding::<S>(&nft, &collection, &ctx_on_b);
    assert_ne!(bind_on_a, bind_on_b, "a different zone gives a different binding");
    let fact = Fact { binding: bind_on_a, not_before: nb, expires_at: expires };
    assert_eq!(check(&fact.encode(), &bind_on_a, Some(nb + 1)), Outcome::Ok);
    assert_eq!(check(&fact.encode(), &bind_on_b, Some(nb + 1)), Outcome::BindingMismatch);

    // Off-chain context: a hashed caller identity and the app's purpose/nonce.
    let ctx_off_a = context_offchain::<S>(&zone_a, &gate, b"basecamp:members-door", b"Check in", &[7; 32]);
    let ctx_off_b = context_offchain::<S>(&zone_b, &gate, b"basecamp:members-door", b"Check in", &[7; 32]);
    assert_ne!(ctx_off_a, ctx_off_b, "the zone is in the off-chain context");
    let bind_off_a = binding::<S>(&nft, &collection, &ctx_off_a);
    let bind_off_b = binding::<S>(&nft, &collection, &ctx_off_b);
    assert_ne!(bind_off_a, bind_off_b, "a different zone gives a different off-chain binding");
    let fact = Fact { binding: bind_off_a, not_before: nb, expires_at: expires };
    assert_eq!(check(&fact.encode(), &bind_off_a, Some(nb + 1)), Outcome::Ok);
    assert_eq!(check(&fact.encode(), &bind_off_b, Some(nb + 1)), Outcome::BindingMismatch);

    // The kind byte also keeps a same-zone on-chain context out of an off-chain check.
    assert_eq!(check(&fact.encode(), &bind_on_a, Some(nb + 1)), Outcome::BindingMismatch);
}
