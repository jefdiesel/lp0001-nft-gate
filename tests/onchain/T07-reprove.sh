#!/bin/bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# T7 (after T1): a second proof from the same holding for another context.
# Pass: the same slot account is rewritten (admission evidence shrinks: rewrite), the first
# context now verifies BINDING_MISMATCH and the second OK.
. "$(dirname "$0")/env.sh"; . "$(dirname "$0")/lib.sh" T07
N1=$(cat $G/run/T01/app-nonce); N2=$(nonce32)
P1=$(python3 -I -c "import json;print(json.load(open('$G/run/T01/proof.json'))['slot_account'])")
run prove $CLI prove --holder h1 --payer h1 --caller-identity basecamp:other-app --purpose "Other" --app-nonce $N2 --out $O/proof.json
expect_ok prove
P2=$(python3 -I -c "import json;print(json.load(open('$O/proof.json'))['slot_account'])")
[ "$P1" = "$P2" ] || { VERDICT=FAIL; note "slot account changed: $P1 -> $P2"; }
SLOT=$(python3 -I -c "import json;print(json.load(open('$O/proof.json'))['slot'])")
run v-first $CLI verify --slot $SLOT --caller-identity basecamp:members-door --purpose "Check in" --app-nonce $N1
expect_text v-first "outcome BINDING_MISMATCH"
run v-second $CLI verify --slot $SLOT --caller-identity basecamp:other-app --purpose "Other" --app-nonce $N2
expect_text v-second "outcome OK"
run decode $CLI decode-tx --hash $(tx_of prove)
note "admission evidence: $(grep '^admission_evidence' $O/decode.out | awk '{print $2}') bytes (T1: $(grep '^admission_evidence' $G/run/T01/log.txt $G/run/T01/decode.out 2>/dev/null | head -1 | awk '{print $2}'))"
finish_test
