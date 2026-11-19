#!/bin/bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# T11 (after T1, T7 and T5/T9): the 32-byte-window comparison of F A2.3. Two proofs from one
# holder (T1, T7) and one from another holder (T5 or T9). Pass: beyond what every Prove shares,
# the same-holder pair shares only windows over the slot account, the slot and public constants.
. "$(dirname "$0")/env.sh"; . "$(dirname "$0")/lib.sh" T11
get() { curl -s -X POST $RPC -H content-type:application/json -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"getTransaction\",\"params\":[\"$1\"]}" > $O/$2.json; }
txf() { grep -oE "^submitted [0-9a-f]{64}" "$1" | tail -1 | cut -d' ' -f2; }
get $(txf $G/run/T01/prove.out) same1
get $(txf $G/run/T07/prove.out) same2
OTHER=$(grep -oE "^submitted [0-9a-f]{64}" $G/run/T05/prove.out $G/run/T09/prove-h5.out 2>/dev/null | tail -1 | grep -oE "[0-9a-f]{64}")
get $OTHER other
SLOT=$(python3 -I -c "import json;print(json.load(open('$G/run/T01/proof.json'))['slot'])")
PDA=$(b58hex $(python3 -I -c "import json;print(json.load(open('$G/run/T01/proof.json'))['slot_account'])"))
GATEH=$(b58hex $(python3 -I -c "import json;print(json.load(open('$GATE_CONFIG'))['gate'])"))
# The fee inbox (8ga8NDaW..., native-balance actor) is a chain-wide constant the wallet adds to
# every privacy-preserving transaction (lez/wallet lib.rs: Actor::native_balance(fee_inbox)).
# Actors are sorted by id, so the inbox sits first when the holder's slot account sorts above
# it and second otherwise: two Proves of one holder then share the windows over "count + inbox"
# that a holder whose slot account sorts the other way does not (run of 2026-11-19: inbox at
# offset 5 in same1/same2, 69 in other). It is a public constant, so it is allowed; the check
# that it really is in all three transactions keeps it from hiding anything else.
FEE_INBOX=7224eab6e02f8b37d841b163beb8b01470a42e73875ecccfcd72182be1b13b5c
for f in same1 same2 other; do
  python3 -I -c "import base64,json,sys;b=base64.b64decode(json.load(open(sys.argv[1]))['result'][0]);sys.exit(bytes.fromhex(sys.argv[2]) not in b)" $O/$f.json $FEE_INBOX \
    || { VERDICT=FAIL; note "the fee inbox is not in $f: it is not the shared constant this allowance assumes"; }
done
run windows python3 -I "$R/tools/linkability.py" $O/same1.json $O/same2.json $O/other.json allow=$PDA,$SLOT,$GATEH,$FEE_INBOX
expect_text windows "^PASS"
finish_test
