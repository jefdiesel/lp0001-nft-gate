// SPDX-License-Identifier: MIT OR Apache-2.0
//! Spec §4.6: "slot_secret: 32 random bytes on first use. It lives only in the shielded
//! gate state. The wallet re-reads it from its own note after a restore."
//!
//! What this repo can check: the Holder state carries the secret in a stable encoding,
//! and the slot and its PDA are derivable from the decoded secret alone. The restore
//! itself is the Names Wallet's job (another repo); no wallet code lives here.

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

fn encoded(state: &GateState) -> Vec<u8> {
    let mut buf = [0_u8; HOLDER_LEN_PENDING];
    let len = state.encode_into(&mut buf);
    buf[..len].to_vec()
}

#[test]
fn a_decoded_holder_state_yields_the_same_slot_and_pda() {
    let gate = [0x6A; 32];
    let secret = [0x37; 32];
    for pending in [None, Some(Pending {
        collection: [0xC0; 32],
        binding: [0xB0; 32],
        not_before: 1_763_500_200_000,
        expires_at: 1_763_500_200_000 + 3_600_000,
    })] {
        let state = GateState::Holder { secret, pending };
        let bytes = encoded(&state);
        // A restored wallet reads this state from its note and decodes it.
        let decoded = GateState::decode(&bytes).expect("holder state decodes");
        let GateState::Holder { secret: recovered, .. } = decoded else {
            panic!("a holder state must decode as a Holder");
        };
        assert_eq!(recovered, secret, "the secret survives the state encoding");
        let derived = slot::<S>(&recovered);
        assert_eq!(derived, slot::<S>(&secret));
        let pda = slot_pda::<S>(&gate, &derived);
        assert_eq!(pda, slot_pda::<S>(&gate, &slot::<S>(&secret)));
        // A different secret gives a different slot: the wallet cannot mix holders.
        assert_ne!(slot::<S>(&[0x38; 32]), derived);
    }
}

#[test]
fn a_fact_state_never_yields_a_holder_secret() {
    let fact = Fact {
        binding: [0xB0; 32],
        not_before: 1_763_500_200_000,
        expires_at: 1_763_500_200_000 + 3_600_000,
    };
    let decoded = GateState::decode(&fact.encode()).expect("fact decodes");
    assert!(
        matches!(decoded, GateState::Fact(_)),
        "a restored wallet must not mistake a public fact for its private holder"
    );
}
