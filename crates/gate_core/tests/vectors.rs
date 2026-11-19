// SPDX-License-Identifier: MIT OR Apache-2.0
//! gate_core against the golden vectors from `tools/gen_vectors.py` (Python standard library).

use gate_core::*;
use serde_json::Value;

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

fn vectors() -> Value {
    let text = include_str!("../../../vectors/gate-vectors.json");
    serde_json::from_str(text).expect("vectors parse")
}

fn hx(v: &Value) -> Vec<u8> {
    let s = v.as_str().expect("hex string");
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex"))
        .collect()
}

fn h32(v: &Value) -> Bytes32 {
    hx(v).try_into().expect("32 bytes")
}

fn num(v: &Value) -> u64 {
    v.as_str().expect("decimal string").parse().expect("u64")
}

#[test]
fn constants_match() {
    let c = &vectors()["constants"];
    assert_eq!(num(&c["QUANT_MS"]), QUANT_MS);
    let ttl: Vec<u64> = c["TTL_MS"].as_array().unwrap().iter().map(num).collect();
    assert_eq!(ttl, TTL_MS.to_vec());
    assert_eq!(hx(&c["DOMAIN_SLOT"]), DOMAIN_SLOT);
    assert_eq!(hx(&c["DOMAIN_CTX"]), DOMAIN_CTX);
    assert_eq!(hx(&c["DOMAIN_BIND"]), DOMAIN_BIND);
    assert_eq!(hx(&c["LEE_PDA_PREFIX"]), LEE_PDA_PREFIX);
    let codes: Vec<&str> = c["codes"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    let ours: Vec<&str> = Outcome::ALL.iter().map(|o| o.name()).collect();
    assert_eq!(codes, ours, "code names and numbering");
}

#[test]
fn derivations_match() {
    let v = vectors();
    let mut n = 0;
    for d in v["derivations"].as_array().unwrap() {
        let name = d["name"].as_str().unwrap();
        if name.starts_with("slot-") {
            let sl = slot::<S>(&h32(&d["secret"]));
            assert_eq!(sl, h32(&d["slot"]), "{name} slot");
            assert_eq!(slot_pda::<S>(&h32(&d["gate"]), &sl), h32(&d["pda"]), "{name} pda");
        } else if name.starts_with("onchain-") {
            let nonce = num(&d["nonce"]);
            let p = payload_onchain::<S>(&h32(&d["submission_id"]), &h32(&d["actor"]), nonce);
            assert_eq!(p, h32(&d["payload"]), "{name} payload");
            let c = context_onchain::<S>(
                &h32(&d["zone_id"]),
                &h32(&d["gate"]),
                &h32(&d["consumer"]),
                &h32(&d["submission_id"]),
                &h32(&d["actor"]),
                nonce,
            );
            assert_eq!(c, h32(&d["context"]), "{name} context");
            assert_eq!(
                binding::<S>(&h32(&d["nft_program"]), &h32(&d["collection"]), &c),
                h32(&d["binding"]),
                "{name} binding"
            );
        } else if name.starts_with("offchain-") {
            let id = hx(&d["caller_identity"]);
            let purpose = hx(&d["purpose"]);
            let nonce = h32(&d["app_nonce"]);
            assert_eq!(audience_offchain::<S>(&id), h32(&d["audience"]), "{name} audience");
            assert_eq!(payload_offchain::<S>(&purpose, &nonce), h32(&d["payload"]), "{name} payload");
            let c = context_offchain::<S>(&h32(&d["zone_id"]), &h32(&d["gate"]), &id, &purpose, &nonce);
            assert_eq!(c, h32(&d["context"]), "{name} context");
            assert_eq!(
                binding::<S>(&h32(&d["nft_program"]), &h32(&d["collection"]), &c),
                h32(&d["binding"]),
                "{name} binding"
            );
        } else {
            panic!("unknown derivation {name}");
        }
        n += 1;
    }
    assert_eq!(n, 10);
}

#[test]
fn observed_chain_pda_matches() {
    // A PDA the throwaway rc3 chain really created (F A2.4 case 1): slot_pda reimplements
    // lee_core's for_public_pda byte for byte.
    let o = &vectors()["observed_pda"];
    assert_eq!(slot_pda::<S>(&h32(&o["program"]), &h32(&o["seed"])), h32(&o["pda"]));
}

#[test]
fn state_encodings_match() {
    for s in vectors()["states"].as_array().unwrap() {
        let bytes = hx(&s["bytes"]);
        let state = match s["kind"].as_str().unwrap() {
            "fact" => GateState::Fact(Fact {
                binding: h32(&s["binding"]),
                not_before: num(&s["not_before"]),
                expires_at: num(&s["expires_at"]),
            }),
            "holder" => GateState::Holder {
                secret: h32(&s["secret"]),
                pending: s.get("collection").map(|c| Pending {
                    collection: h32(c),
                    binding: h32(&s["binding"]),
                    not_before: num(&s["not_before"]),
                    expires_at: num(&s["expires_at"]),
                }),
            },
            other => panic!("kind {other}"),
        };
        let mut buf = [0_u8; HOLDER_LEN_PENDING];
        let len = state.encode_into(&mut buf);
        assert_eq!(&buf[..len], &bytes[..], "{} encode", s["name"]);
        assert_eq!(GateState::decode(&bytes), Ok(state), "{} decode", s["name"]);
    }
}

#[test]
fn check_cases_match() {
    let v = vectors();
    let cases = v["check_cases"].as_array().unwrap();
    for c in cases {
        let name = c["name"].as_str().unwrap();
        let state = hx(&c["state"]);
        let expected = h32(&c["expected_binding"]);
        let now = c["now"].as_str().map(|s| s.parse::<u64>().unwrap());
        let want = Outcome::from_name(c["outcome"].as_str().unwrap()).unwrap();
        let want_onchain = Outcome::from_name(c["onchain_outcome"].as_str().unwrap()).unwrap();
        assert_eq!(check(&state, &expected, now), want, "{name}");
        assert_eq!(check(&state, &expected, None), want_onchain, "{name} on chain");
    }
    assert_eq!(cases.len(), 26);
}

#[cfg(feature = "sha2")]
#[test]
fn feature_hasher_equals_test_hasher() {
    let parts: [&[u8]; 3] = [b"a", b"", b"bc"];
    assert_eq!(<gate_core::Sha2 as Sha256>::hash(&parts), S::hash(&parts));
}
