#!/bin/bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# T10 (after T8): the consumer on chain and the library agree, code by code, on the same slot
# bytes. Native half: consumer_program's t10 test and the JS tests run the shared vector file
# (T00). Chain half, from T8's runs: OK (Act applied / verify OK), BINDING_MISMATCH (B's Act /
# verify as B), NO_FACT (empty slot / verify), EXPIRED (sequencer refused / verify EXPIRED).
. "$(dirname "$0")/env.sh"; . "$(dirname "$0")/lib.sh" T10
T8=$G/run/T08
SLOT=$(python3 -I -c "import json;print(json.load(open('$T8/proof.json'))['slot'])")
run v-B $CLI verify --slot $SLOT --actor actB --nonce 0
expect_text v-B "outcome BINDING_MISMATCH"
grep -q "consumer: BINDING_MISMATCH" $T8/act-B.out || { VERDICT=FAIL; note "consumer did not say BINDING_MISMATCH for B"; }
grep -q "outcome OK" $T8/both-ways.out && grep -q "consumer nonce 0 -> 1" $T8/act-A.out || { VERDICT=FAIL; note "OK pair missing"; }
EMPTY=$(grep -oE -- "--slot [0-9a-f]{64}" $T8/act-empty.out | head -1 | cut -d' ' -f2)
run v-empty $CLI verify --slot $EMPTY --actor actA --nonce 1
expect_text v-empty "outcome NO_FACT"
grep -q "consumer: NO_FACT" $T8/act-empty.out || note "on chain the empty slot was refused before the consumer: $(grep -m1 -E 'FAILED|detail' $T8/act-empty.out) (record: NO_FACT reachable only off chain)"
grep -q "outcome EXPIRED" $T8/v-late.out && [ "$(cat $T8/act-late.rc)" != 0 ] || { VERDICT=FAIL; note "EXPIRED pair missing"; }
finish_test
