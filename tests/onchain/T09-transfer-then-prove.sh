#!/bin/bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# T9 (after T5): transfer h3's copy to the fresh h5, then h5 proves. Pass: a second slot
# appears; h3's earlier fact (T5) still verifies OK until it expires (limit L2); h3 can no
# longer prove (NOT_A_MEMBER).
. "$(dirname "$0")/env.sh"; . "$(dirname "$0")/lib.sh" T09
if [ ! -s $G/run/T05/proof.json ]; then   # T5 refused the gas payer: h3 proves for itself
  mkdir -p $G/run/T05; N5=$(nonce32); echo $N5 > $G/run/T05/app-nonce
  run pre $CLI prove --holder h3 --payer h3 --caller-identity t5 --purpose T5 --app-nonce $N5 --out $G/run/T05/proof.json
fi
run transfer $CLI nft transfer --from h3 --to h5 --payer h3
expect_ok transfer
run pending $CLI nft pending
run read-h5 $CLI nft read --account h5
expect_text read-h5 "owned: true"
N=$(nonce32)
run prove-h5 $CLI prove --holder h5 --payer h5 --caller-identity t9 --purpose T9 --app-nonce $N --out $O/proof.json
expect_ok prove-h5
S5=$(python3 -I -c "import json;print(json.load(open('$O/proof.json'))['slot_account'])")
S3=$(python3 -I -c "import json;print(json.load(open('$G/run/T05/proof.json'))['slot_account'])" 2>/dev/null)
[ -n "$S3" ] && [ "$S3" != "$S5" ] || { VERDICT=FAIL; note "no second slot (h3 $S3, h5 $S5)"; }
SLOT3=$(python3 -I -c "import json;print(json.load(open('$G/run/T05/proof.json'))['slot'])" 2>/dev/null)
run v-old $CLI verify --slot $SLOT3 --caller-identity t5 --purpose T5 --app-nonce $(cat $G/run/T05/app-nonce)
expect_text v-old "outcome OK"
run h3-again $CLI prove --holder h3 --payer h3 --caller-identity t9 --purpose again --app-nonce $(nonce32)
expect_refused h3-again NOT_A_MEMBER
note "L2 documented: the old fact stays live after the transfer"
finish_test
