#!/bin/bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# T5: a separate private gas payer, the holder's two actors both mentioned as authorised.
# Records: applied, or the exact refusal. Decides spec §4.3 / owner question Q5.
# Pass (as a test) when it either applies with neither id in the clear, or refuses with a
# recorded reason; the verdict line says which.
. "$(dirname "$0")/env.sh"; . "$(dirname "$0")/lib.sh" T05
N=$(nonce32); echo $N > $O/app-nonce
run prove $CLI prove --holder h3 --payer gas --caller-identity t5 --purpose T5 --app-nonce $N --out $O/proof.json
if [ "$(cat $O/prove.rc)" = 0 ]; then
  TX=$(tx_of prove)
  curl -s -X POST $RPC -H content-type:application/json -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"getTransaction\",\"params\":[\"$TX\"]}" > $O/raw.json
  python3 -I "$R/tools/search_tx.py" $O/raw.json holder=$(b58hex $(acct h3)) payer=$(b58hex $(acct gas)) > $O/search.txt
  grep -q FOUND $O/search.txt && { VERDICT=FAIL; note "an id in the clear: $(grep FOUND $O/search.txt)"; }
  note "APPLIED: a separate private gas payer works (Q5 may switch the default)"
else
  VERDICT=PASS-REFUSED; note "REFUSED: $(grep -m1 -E 'FAILED|detail' $O/prove.out)"
fi
finish_test
