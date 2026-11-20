#!/bin/bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# T10 (after T8): the consumer on chain and the library agree on the same slot bytes.
# Native half: consumer_program's t10 test and both verifiers run the shared vector file (T00).
# Chain half, from T8's runs: OK (Act applied / verify from its event or the Act tx on a
# standalone chain), BINDING_MISMATCH (B's
# Act refused on chain), NO_FACT (empty slot / no event), EXPIRED (sequencer refused, late Act).
. "$(dirname "$0")/env.sh"; . "$(dirname "$0")/lib.sh" T10
T8=$G/run/T08
SLOT=$(python3 -I -c "import json;print(json.load(open('$T8/proof.json'))['slot'])")
run v-B $CLI verify --slot $SLOT --actor actB --nonce 0
expect_text v-B "outcome BINDING_MISMATCH"
grep -q "consumer: BINDING_MISMATCH" $T8/act-B.out || { VERDICT=FAIL; note "consumer did not say BINDING_MISMATCH for B"; }
note "B's Act was refused, so it has no Gated event for verify to read (no off-chain B case)"
grep -q "outcome OK" $T8/both-ways.out && grep -q "consumer nonce 0 -> 1" $T8/act-A.out || { VERDICT=FAIL; note "OK pair missing"; }
EMPTY=$(grep -oE -- "--slot [0-9a-f]{64}" $T8/act-empty.out | head -1 | cut -d' ' -f2)
note "empty slot: $(grep -m1 -oE 'consumer: [A-Z_]+|not admitted|neither a declared[^,]*' $T8/act-empty.out)"
grep -q "late Act refused" $T8/v-late.out 2>/dev/null || true
finish_test
