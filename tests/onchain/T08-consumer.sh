#!/bin/bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# T8: the reference consumer. Pass: another account's Act on A's fact fails BINDING_MISMATCH;
# A's Act applies once (nonce 0 -> 1, the gate PDA read from a public transaction); A's second
# Act fails BINDING_MISMATCH; an Act on a never-written slot fails (NO_FACT, or LEZ's own
# refusal, recorded); an Act after expiry is refused by the sequencer. Records the gas.
# Also "same proof, both ways": the off-chain library on the on-chain context says OK.
. "$(dirname "$0")/env.sh"; . "$(dirname "$0")/lib.sh" T08
run prove $CLI prove --holder h1 --payer h1 --onchain-actor actA --out $O/proof.json
expect_ok prove
SLOT=$(python3 -I -c "import json;print(json.load(open('$O/proof.json'))['slot'])")
run act-B $CLI act --actor actB --slot $SLOT
expect_refused act-B "consumer: BINDING_MISMATCH"
run act-A $CLI act --actor actA --slot $SLOT
expect_ok act-A
expect_text act-A "consumer nonce 0 -> 1"
run gated $CLI gated --tx $(tx_of act-A)
expect_text gated "reverted false"
run decode-act $CLI decode-tx --hash $(tx_of act-A)
note "Act gas: $(grep '^fee ' $O/decode-act.out)"
run both-ways $CLI verify --slot $SLOT --actor actA --nonce 0
expect_text both-ways "outcome OK"
run act-A2 $CLI act --actor actA --slot $SLOT
expect_refused act-A2 "consumer: BINDING_MISMATCH"
run act-empty $CLI act --actor actA --slot $(nonce32)
[ "$(cat $O/act-empty.rc)" != 0 ] || { VERDICT=FAIL; note "Act on an empty slot applied"; }
note "empty slot: $(grep -m1 -oE 'consumer: [A-Z_]+|not admitted|neither a declared[^,]*' $O/act-empty.out)"
# Late Act: a fact whose window closes within 10 minutes, then wait past it.
NOW=$(chain_ms); NB=$(( $(grid $NOW) - 3000000 ))
run prove-late $CLI prove --holder h1 --payer h1 --onchain-actor actA --test-not-before $NB --test-ttl-class 0 --out $O/late.json
expect_ok prove-late
EXP=$(python3 -I -c "import json;print(json.load(open('$O/late.json'))['expires_at'])")
LSLOT=$(python3 -I -c "import json;print(json.load(open('$O/late.json'))['slot'])")
note "waiting for chain time >= $EXP (up to 11 min)"
for i in $(seq 1 140); do [ "$(chain_ms)" -ge "$EXP" ] && break; sleep 5; done
run act-late $CLI act --actor actA --slot $LSLOT
[ "$(cat $O/act-late.rc)" != 0 ] || { VERDICT=FAIL; note "a late Act applied"; }
note "late Act refused at: $(grep -m1 -E 'FAILED|detail|not included|Validity' $O/act-late.out)"
run v-late $CLI verify --slot $LSLOT --actor actA --nonce 1
expect_text v-late "outcome EXPIRED"
finish_test
