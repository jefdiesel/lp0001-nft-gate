#!/bin/bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# T8: the reference consumer. Pass: another account's Act on A's fact fails BINDING_MISMATCH;
# A's Act applies once (nonce 0 -> 1, the gate PDA read from a public transaction); A's second
# Act fails BINDING_MISMATCH; an Act on a never-written slot fails (NO_FACT, or LEZ's own
# refusal, recorded); an Act after expiry is refused by the sequencer. Records the gas.
# Also "same proof, both ways": the on-chain use is re-checked from its Gated event.
# The RPC used by `verify` must serve getEvents (or pass --events-url with an indexer).
. "$(dirname "$0")/env.sh"; . "$(dirname "$0")/lib.sh" T08
run prove $CLI prove --holder h1 --payer h1 --onchain-actor actA --out $O/proof.json
expect_ok prove
SLOT=$(python3 -I -c "import json;print(json.load(open('$O/proof.json'))['slot'])")
run act-B $CLI act --actor actB --slot $SLOT
expect_refused act-B "consumer: BINDING_MISMATCH"
run act-A $CLI act --actor actA --slot $SLOT
expect_ok act-A
# Once only: A's consumer nonce goes up by exactly one. It is 0 on a fresh consumer; a re-run on
# the same deployment starts where the last run left it (run of 2026-11-19: 1 -> 2).
N0=$(grep -oE "^Act by [^ ]+ at consumer nonce [0-9]+" $O/act-A.out | grep -oE "[0-9]+$")
if [ -z "$N0" ]; then VERDICT=FAIL; note "act-A printed no nonce before the Act";
else expect_text act-A "consumer nonce $N0 -> $((N0 + 1));"; note "A's consumer nonce before the Act: $N0"; fi
run gated $CLI gated --tx $(tx_of act-A)
expect_text gated "reverted false"
run decode-act $CLI decode-tx --hash $(tx_of act-A)
note "Act gas: $(grep '^fee ' $O/decode-act.out)"
run both-ways $CLI verify --slot $SLOT --tx-hash $(tx_of act-A)
if events_served; then
  expect_text both-ways "outcome OK"
elif has both-ways "outcome RPC_UNAVAILABLE"; then
  # A standalone sequencer (the 3310 kit) has no getEvents: that is the LEZ indexer's method,
  # and the indexer follows a Bedrock node, which this chain does not post to.
  blocked "both-ways: $RPC has no getEvents (no LEZ indexer on this chain); verify --tx-hash cannot read the Gated event here (give --events-url on a chain with an indexer)"
else
  VERDICT=FAIL; note "both-ways: $RPC has no getEvents, yet verify did not say RPC_UNAVAILABLE"
fi
run act-A2 $CLI act --actor actA --slot $SLOT
expect_refused act-A2 "consumer: BINDING_MISMATCH"
run act-empty $CLI act --actor actA --slot $(nonce32)
[ "$(cat $O/act-empty.rc)" != 0 ] || { VERDICT=FAIL; note "Act on an empty slot applied"; }
note "empty slot: $(grep -m1 -oE 'consumer: [A-Z_]+|not admitted|neither a declared[^,]*' $O/act-empty.out)"
# Late Act: a fact whose window closes within 10 minutes, then wait past it.
# ttl_class 0 is one hour, so the window [NB, NB + 1 h) closes at the next 10-minute mark. If
# that is under 3 minutes away, the Prove itself may land after it (run of 2026-11-19: 11 s away,
# "not included"), so close it one mark later (10-13 minutes away).
NOW=$(chain_ms); NB=$(( $(grid $NOW) - 3000000 ))
[ $(( NB + 3600000 - NOW )) -ge 180000 ] || NB=$(( NB + 600000 ))
run prove-late $CLI prove --holder h1 --payer h1 --onchain-actor actA --test-not-before $NB --test-ttl-class 0 --out $O/late.json
expect_ok prove-late
EXP=$(python3 -I -c "import json;print(json.load(open('$O/late.json'))['expires_at'])" 2>/dev/null)
LSLOT=$(python3 -I -c "import json;print(json.load(open('$O/late.json'))['slot'])" 2>/dev/null)
if [ -z "$EXP" ] || [ -z "$LSLOT" ]; then
  # without the late fact the late Act below would be refused for a missing slot, not for expiry
  VERDICT=FAIL; note "no late fact (prove-late: $(grep -m1 -E 'FAILED' $O/prove-late.out)): the late Act was not tried"
else
  note "waiting for chain time >= $EXP (up to 15 min)"
  for i in $(seq 1 180); do [ "$(chain_ms)" -ge "$EXP" ] && break; sleep 5; done
  [ "$(chain_ms)" -ge "$EXP" ] || { VERDICT=FAIL; note "chain time never reached expires_at $EXP: the late Act would not be late"; }
  run act-late $CLI act --actor actA --slot $LSLOT
  [ "$(cat $O/act-late.rc)" != 0 ] || { VERDICT=FAIL; note "a late Act applied"; }
  note "late Act refused at: $(grep -m1 -E 'FAILED|detail|not included|Validity' $O/act-late.out)"
fi
# A late Act is refused by the sequencer and has no Gated event to re-check.
finish_test
