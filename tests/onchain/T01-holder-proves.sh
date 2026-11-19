#!/bin/bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# T1: a holder proves. Pass: applied; the fact reads back OK; the decoded transaction holds no
# holder id, no payer id, no collection id, no slot secret and no context in the clear; the
# window is not empty; the outcome check passed (it ran on the real proven transaction).
. "$(dirname "$0")/env.sh"; . "$(dirname "$0")/lib.sh" T01
N=$(nonce32)
echo $N > $O/app-nonce
run prove $CLI prove --holder h1 --payer h1 --caller-identity basecamp:members-door --purpose "Check in" --app-nonce $N --out $O/proof.json
expect_ok prove
expect_text prove "check 6 outcome: passed"
expect_text prove "read back: OK"
TX=$(tx_of prove)
run decode $CLI decode-tx --hash $TX
curl -s -X POST $RPC -H content-type:application/json -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"getTransaction\",\"params\":[\"$TX\"]}" > $O/raw.json
run slot $CLI slot --holder h1 --test-show-secret
SECRET=$(awk '/^TEST ONLY secret/{print $4}' $O/slot.out)
CTX=$(python3 -I - <<PY
import hashlib,json
c=json.load(open("$GATE_CONFIG"))
import sys; sys.path.insert(0,"$R/tools")
from b58 import to_hex
h=lambda *p: hashlib.sha256(b"".join(p)).digest()
gate=bytes.fromhex(to_hex(c["gate"])); zone=bytes.fromhex(c["zone_id"])
aud=h(b"basecamp:members-door"); pay=h(b"Check in", bytes.fromhex("$N"))
print(h(b"lp0001-gate/v1/ctx", b"\x02", zone, gate, aud, pay).hex())  # off-chain kind 0x02
PY
)
python3 -I "$R/tools/search_tx.py" $O/raw.json \
  holder=$(b58hex $(acct h1)) collection=$(b58hex $(acct defn)) secret=$SECRET context=$CTX > $O/search.txt
cat $O/search.txt >> $LOG
grep -q "FOUND" $O/search.txt && { VERDICT=FAIL; note "a private value is in the clear: $(grep FOUND $O/search.txt | tr '\n' ' ')"; }
expect_no_text decode "validity timestamps \[None, None)"
finish_test
