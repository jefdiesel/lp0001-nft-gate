#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Golden vectors for gate_core, from the Python standard library only.

An independent second implementation of the spec's derivations (LP0001-OWNERSHIP-PROOF-SPEC.md
§2.2-§2.4, §3.3). The Rust crate, the JS package and the wasm build are checked against the
file this writes; none of them is used to make it.

Usage:  python3 -I tools/gen_vectors.py            # writes vectors/gate-vectors.json
        python3 -I tools/gen_vectors.py --check    # exits 1 if the file is out of date
"""
import hashlib
import json
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "..", "vectors", "gate-vectors.json")

QUANT_MS = 600_000
TTL_MS = [3_600_000, 21_600_000, 86_400_000]
DOMAIN_SLOT = b"lp0001-gate/v1/slot"
DOMAIN_CTX = b"lp0001-gate/v1/ctx"
DOMAIN_BIND = b"lp0001-gate/v1/bind"
LEE_PDA_PREFIX = b"/LEE/v0.2/AccountId/PDA/" + b"\x00" * 8
KIND_ONCHAIN = 0x01
KIND_OFFCHAIN = 0x02
B58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"

CODES = ["OK", "NO_FACT", "MALFORMED", "BINDING_MISMATCH", "NOT_YET_VALID", "EXPIRED",
         "NOT_AUTHORIZED", "RPC_UNAVAILABLE", "NOT_A_MEMBER"]


def h(*parts):
    m = hashlib.sha256()
    for p in parts:
        m.update(p)
    return m.digest()


def label(name):
    """A deterministic 32-byte test input."""
    return h(b"lp0001-gate/test-vector/", name.encode())


def b58enc(b):
    n = int.from_bytes(b, "big")
    out = ""
    while n:
        n, r = divmod(n, 58)
        out = B58[r] + out
    pad = len(b) - len(b.lstrip(b"\x00"))
    return "1" * pad + out


def b58dec(s):
    n = 0
    for c in s:
        n = n * 58 + B58.index(c)
    raw = n.to_bytes(32, "big")
    return raw


def u64(v):
    return struct.pack("<Q", v)


def slot(secret):
    return h(DOMAIN_SLOT, secret)


def slot_pda(gate, slot_):
    return h(LEE_PDA_PREFIX, gate, slot_)


def context(kind, zone, gate, audience, payload):
    return h(DOMAIN_CTX, bytes([kind]), zone, gate, audience, payload)


def binding(nft, collection, ctx):
    return h(DOMAIN_BIND, nft, collection, ctx)


def payload_onchain(submission_id, actor, nonce):
    return h(submission_id, actor, u64(nonce))


def payload_offchain(purpose, nonce):
    return h(purpose, nonce)


def audience_offchain(identity):
    return h(identity)


def fact_bytes(binding_, nb, exp, tag=1, version=1):
    return bytes([tag, version]) + binding_ + u64(nb) + u64(exp)


def holder_bytes(secret, pending=None):
    out = bytes([0, 1]) + secret
    if pending is None:
        return out + b"\x00"
    collection, binding_, nb, exp = pending
    return out + b"\x01" + collection + binding_ + u64(nb) + u64(exp)


def decode_fact(b):
    """Strict decode, the same rules as gate_core::Fact::decode. None if not a v1 fact."""
    if len(b) != 50 or b[0] != 1 or b[1] != 1:
        return None
    nb = struct.unpack("<Q", b[34:42])[0]
    exp = struct.unpack("<Q", b[42:50])[0]
    if nb % QUANT_MS != 0 or exp < nb or (exp - nb) not in TTL_MS:
        return None
    return b[2:34], nb, exp


def check(state, expected, now):
    if len(state) == 0:
        return "NO_FACT"
    f = decode_fact(state)
    if f is None:
        return "MALFORMED"
    b, nb, exp = f
    if b != expected:
        return "BINDING_MISMATCH"
    if now is not None:
        if now < nb:
            return "NOT_YET_VALID"
        if now >= exp:
            return "EXPIRED"
    return "OK"


def x(b):
    return b.hex()


def build():
    nft = label("nft-program")
    gate = label("gate-program")
    consumer = label("consumer-program")
    zone = label("zone-channel-id")
    zone2 = label("zone-channel-id-after-reset")
    coll = label("collection-C")
    coll2 = label("collection-D")
    sub = label("submission-id")
    actor = label("acting-account-A")
    actor2 = label("acting-account-B")
    secret = label("slot-secret-1")
    secret2 = label("slot-secret-2")
    app_nonce = label("app-nonce-1")
    identity = b"basecamp:members-door"
    purpose = "Check in this app".encode()

    derivations = []

    def add(name, **kv):
        derivations.append({"name": name, **{k: (x(v) if isinstance(v, bytes) else v) for k, v in kv.items()}})

    for i, s in enumerate([secret, secret2, b"\x00" * 32, b"\xff" * 32]):
        sl = slot(s)
        pda = slot_pda(gate, sl)
        add(f"slot-{i}", secret=s, gate=gate, slot=sl, pda=pda, pda_b58=b58enc(pda))

    for nonce in [0, 1, 2**32, 2**64 - 1]:
        p = payload_onchain(sub, actor, nonce)
        c = context(KIND_ONCHAIN, zone, gate, consumer, p)
        add(f"onchain-nonce-{nonce}", zone_id=zone, gate=gate, consumer=consumer, submission_id=sub,
            actor=actor, nonce=str(nonce), payload=p, context=c,
            binding=binding(nft, coll, c), nft_program=nft, collection=coll)

    aud = audience_offchain(identity)
    p = payload_offchain(purpose, app_nonce)
    c = context(KIND_OFFCHAIN, zone, gate, aud, p)
    add("offchain-1", zone_id=zone, gate=gate, caller_identity=x(identity), purpose=x(purpose),
        app_nonce=app_nonce, audience=aud, payload=p, context=c, nft_program=nft, collection=coll,
        binding=binding(nft, coll, c))
    add("offchain-empty-purpose", zone_id=zone, gate=gate, caller_identity=x(identity), purpose="",
        app_nonce=app_nonce, audience=aud, payload=payload_offchain(b"", app_nonce),
        context=context(KIND_OFFCHAIN, zone, gate, aud, payload_offchain(b"", app_nonce)),
        nft_program=nft, collection=coll,
        binding=binding(nft, coll, context(KIND_OFFCHAIN, zone, gate, aud, payload_offchain(b"", app_nonce))))

    # The probe v2 PDA OBSERVED on the throwaway chain (F A2.4 case 1,
    # job-a/run/a2w-24/summary.txt): pins for_public_pda and base58 to real chain output.
    probe = b58dec("6BZokuzhSs2Lex3PCDfUJX1reNZpCnEg1cZ79vhz3gbm")
    gasdef = bytes.fromhex("67a45186f853626cfcd8f2f79534774ac8b54a66f36bb9c723bb948a72c13eb3")
    c1 = bytes.fromhex("d584c5c5819f06f9eac54fdb083b9a1c953e6f1b25de42a16b51ef26d7fa8bae")
    seed = h(b"nft-probe/v1/fact/", gasdef, c1)
    observed_pda = slot_pda(probe, seed)
    assert b58enc(observed_pda) == "2n2NPDMdqV6XDUwXaGnYcjKnUnsm5Dbx4ovUPhReRQxx", b58enc(observed_pda)
    observed = {
        "source": "OBSERVED, LP0001-JOB-A-FINDINGS.md A2.4 case 1 (probe v2 AskPublish, block 2506)",
        "program_b58": "6BZokuzhSs2Lex3PCDfUJX1reNZpCnEg1cZ79vhz3gbm",
        "program": x(probe), "seed": x(seed), "pda": x(observed_pda),
        "pda_b58": "2n2NPDMdqV6XDUwXaGnYcjKnUnsm5Dbx4ovUPhReRQxx",
    }

    # State encodings.
    b1 = binding(nft, coll, context(KIND_ONCHAIN, zone, gate, consumer, payload_onchain(sub, actor, 0)))
    nb = 1_763_500_200_000  # on the 10-minute grid
    assert nb % QUANT_MS == 0
    states = [
        {"name": "fact-ttl0", "kind": "fact", "binding": x(b1), "not_before": str(nb),
         "expires_at": str(nb + TTL_MS[0]), "bytes": x(fact_bytes(b1, nb, nb + TTL_MS[0]))},
        {"name": "fact-ttl2", "kind": "fact", "binding": x(b1), "not_before": str(nb),
         "expires_at": str(nb + TTL_MS[2]), "bytes": x(fact_bytes(b1, nb, nb + TTL_MS[2]))},
        {"name": "holder-idle", "kind": "holder", "secret": x(secret), "bytes": x(holder_bytes(secret))},
        {"name": "holder-pending", "kind": "holder", "secret": x(secret), "collection": x(coll),
         "binding": x(b1), "not_before": str(nb), "expires_at": str(nb + TTL_MS[1]),
         "bytes": x(holder_bytes(secret, (coll, b1, nb, nb + TTL_MS[1])))},
    ]

    # check() cases. Each names the on-chain code too (no time: the sequencer enforces it).
    exp0 = nb + TTL_MS[0]
    good = fact_bytes(b1, nb, exp0)
    other_ctx = binding(nft, coll, context(KIND_ONCHAIN, zone, gate, consumer, payload_onchain(sub, actor, 1)))
    cases = []

    def case(name, state, expected, now, why):
        cases.append({"name": name, "state": x(state), "expected_binding": x(expected),
                      "now": None if now is None else str(now),
                      "outcome": check(state, expected, now),
                      "onchain_outcome": check(state, expected, None), "why": why})

    case("ok-mid", good, b1, nb + 1000, "current fact, right binding")
    case("ok-at-start", good, b1, nb, "window is [not_before, expires_at): start included")
    case("ok-last-ms", good, b1, exp0 - 1, "last millisecond inside")
    case("ok-no-time", good, b1, None, "on-chain form: no clock")
    case("expired-at-end", good, b1, exp0, "end excluded")
    case("expired-later", good, b1, exp0 + 10**9, "long after")
    case("not-yet-valid", good, b1, nb - 1, "one ms early")
    case("no-fact", b"", b1, nb, "empty slot")
    case("no-fact-no-time", b"", b1, None, "empty slot on chain")
    case("mismatch-other-collection", good, binding(nft, coll2, context(KIND_ONCHAIN, zone, gate, consumer, payload_onchain(sub, actor, 0))), nb, "verifier gates on collection D")
    case("mismatch-other-context-nonce", good, other_ctx, nb, "verifier's nonce moved on (Act again)")
    case("mismatch-other-actor", good, binding(nft, coll, context(KIND_ONCHAIN, zone, gate, consumer, payload_onchain(sub, actor2, 0))), nb, "another account points the consumer at the fact")
    case("mismatch-other-zone", good, binding(nft, coll, context(KIND_ONCHAIN, zone2, gate, consumer, payload_onchain(sub, actor, 0))), nb, "zone reset / other zone")
    case("mismatch-offchain-vs-onchain", good, binding(nft, coll, context(KIND_OFFCHAIN, zone, gate, consumer, payload_onchain(sub, actor, 0))), nb, "kind byte differs")
    case("mismatch-other-nft-program", good, binding(label("look-alike-nft"), coll, context(KIND_ONCHAIN, zone, gate, consumer, payload_onchain(sub, actor, 0))), nb, "look-alike NFT program")
    case("mismatch-superseded", fact_bytes(other_ctx, nb, exp0), b1, nb + 1, "a newer proof for another context overwrote the slot")
    case("mismatch-beats-expired", fact_bytes(other_ctx, nb, exp0), b1, exp0 + 5, "binding is checked before time")
    case("malformed-holder-state", holder_bytes(secret), b1, nb, "a Holder state is not a Fact")
    case("malformed-truncated", good[:49], b1, nb, "49 bytes")
    case("malformed-extended", good + b"\x00", b1, nb, "51 bytes")
    case("malformed-tag", fact_bytes(b1, nb, exp0, tag=2), b1, nb, "unknown tag")
    case("malformed-version", fact_bytes(b1, nb, exp0, version=2), b1, nb, "version 2")
    case("malformed-off-grid", fact_bytes(b1, nb + 1, exp0 + 1), b1, nb + 2, "not_before off the grid")
    case("malformed-bad-ttl", fact_bytes(b1, nb, nb + 1_000), b1, nb + 1, "window not one of the menu")
    case("malformed-reversed", fact_bytes(b1, exp0, nb), b1, nb, "expires before it starts")
    case("malformed-one-byte", b"\x01", b1, None, "one byte")

    for c in cases:
        assert c["outcome"] in CODES

    # T10 agreement cases, under the consumer crate's native-test constants
    # (consumer_program: GATE 0x6A.., NFT_PROGRAM 0xAA.., COLLECTION 0xC0.., SUBMISSION_ID
    # 0x5B.., ZONE_ID 0x2E..; the test registers the consumer at 0xCC.., acting account A at
    # 0xA1.., nonce 0). The same bytes go to the consumer's reply handler and to check().
    T = lambda b: bytes([b]) * 32
    t_gate, t_nft, t_coll, t_sub, t_zone, t_cons, t_a, t_b = (T(0x6A), T(0xAA), T(0xC0), T(0x5B),
                                                               T(0x2E), T(0xCC), T(0xA1), T(0xB2))
    t_bind = binding(t_nft, t_coll, context(KIND_ONCHAIN, t_zone, t_gate, t_cons, payload_onchain(t_sub, t_a, 0)))
    t_bind_n1 = binding(t_nft, t_coll, context(KIND_ONCHAIN, t_zone, t_gate, t_cons, payload_onchain(t_sub, t_a, 1)))
    t_bind_b = binding(t_nft, t_coll, context(KIND_ONCHAIN, t_zone, t_gate, t_cons, payload_onchain(t_sub, t_b, 0)))
    t_bind_coll = binding(t_nft, T(0xC1), context(KIND_ONCHAIN, t_zone, t_gate, t_cons, payload_onchain(t_sub, t_a, 0)))
    consumer_cases = []

    def ccase(name, state, why):
        consumer_cases.append({"name": name, "state": x(state), "expected_binding": x(t_bind),
                               "onchain_outcome": check(state, t_bind, None), "why": why})

    ccase("ok", fact_bytes(t_bind, nb, nb + TTL_MS[0]), "the fact for A at nonce 0")
    ccase("ok-ttl1", fact_bytes(t_bind, nb, nb + TTL_MS[1]), "other TTL")
    ccase("no-fact", b"", "empty slot")
    ccase("mismatch-nonce-1", fact_bytes(t_bind_n1, nb, nb + TTL_MS[0]), "a fact for nonce 1")
    ccase("mismatch-actor-b", fact_bytes(t_bind_b, nb, nb + TTL_MS[0]), "a fact for account B")
    ccase("mismatch-collection", fact_bytes(t_bind_coll, nb, nb + TTL_MS[0]), "a fact for another collection")
    ccase("malformed-holder", holder_bytes(secret), "a Holder state")
    ccase("malformed-short", fact_bytes(t_bind, nb, nb + TTL_MS[0])[:40], "40 bytes")
    ccase("malformed-version", fact_bytes(t_bind, nb, nb + TTL_MS[0], version=0), "version 0")
    ccase("malformed-bad-ttl", fact_bytes(t_bind, nb, nb + 7), "window not on the menu")

    return {
        "generator": "tools/gen_vectors.py (Python standard library only)",
        "spec": "LP0001-OWNERSHIP-PROOF-SPEC.md 2026-11-18, sections 2.2-2.4, 3.3",
        "constants": {
            "QUANT_MS": str(QUANT_MS), "TTL_MS": [str(t) for t in TTL_MS],
            "DOMAIN_SLOT": x(DOMAIN_SLOT), "DOMAIN_CTX": x(DOMAIN_CTX), "DOMAIN_BIND": x(DOMAIN_BIND),
            "LEE_PDA_PREFIX": x(LEE_PDA_PREFIX), "KIND_ONCHAIN": KIND_ONCHAIN, "KIND_OFFCHAIN": KIND_OFFCHAIN,
            "codes": CODES,
        },
        "derivations": derivations,
        "observed_pda": observed,
        "states": states,
        "check_cases": cases,
        "consumer_cases": consumer_cases,
    }


def main():
    text = json.dumps(build(), indent=1, sort_keys=False) + "\n"
    if "--check" in sys.argv:
        with open(OUT) as f:
            if f.read() != text:
                print("vectors/gate-vectors.json is out of date: run tools/gen_vectors.py")
                sys.exit(1)
        print("vectors up to date")
        return
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    with open(OUT, "w") as f:
        f.write(text)
    print(f"wrote {os.path.relpath(OUT)}: derivations, states, check cases")


if __name__ == "__main__":
    main()
