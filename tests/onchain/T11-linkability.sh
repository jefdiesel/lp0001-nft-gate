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
run windows python3 -I "$R/tools/linkability.py" $O/same1.json $O/same2.json $O/other.json allow=$PDA,$SLOT,$GATEH
expect_text windows "^PASS"
finish_test
