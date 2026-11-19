#!/bin/bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# T15 (after T1): same-transaction replay. Spec §2.8 point 1: the identical private
# Prove sent twice is refused because its nullifier is already in LEZ state. This is
# CHAIN-level: a native gate_program test has no nullifier set, so it cannot show it.
# Pass: resubmitting T01's exact transaction bytes is refused ("Nullifier already seen"
# or never included), and the slot's fact is unchanged. NOT RUN in Job I (no chains).
. "$(dirname "$0")/env.sh"; . "$(dirname "$0")/lib.sh" T15
T1=$G/run/T01
HASH=$(grep -oE "^submitted [0-9a-f]{64}" $T1/prove.out | tail -1 | cut -d' ' -f2)
[ -n "$HASH" ] || { VERDICT=FAIL; note "no Prove tx hash in T01"; finish_test; }
SLOT=$(python3 -I -c "import json;print(json.load(open('$T1/proof.json'))['slot'])")
BEFORE=$($CLI read-slot --slot "$SLOT" 2>/dev/null | grep '^state' || true)

# getTransaction returns the raw bytes; sendTransaction submits the identical transaction.
RAW=$(curl -s -X POST $RPC -H content-type:application/json \
  -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"getTransaction\",\"params\":[\"$HASH\"]}" \
  | python3 -I -c "import sys,json; print(json.load(sys.stdin)['result'][0])")
run replay curl -s -X POST $RPC -H content-type:application/json \
  -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"sendTransaction\",\"params\":[\"$RAW\"]}"
# A refused replay returns a JSON-RPC error; an accepted one is a chain-level FAIL.
note "replay response: $(head -c 300 $O/replay.out)"
if has replay '"error"\|Nullifier already seen\|invalid transaction'; then
  note "chain refused the repeated transaction"
else
  VERDICT=FAIL; note "the identical transaction was accepted again (nullifier check missing)"
fi
AFTER=$($CLI read-slot --slot "$SLOT" 2>/dev/null | grep '^state' || true)
[ "$BEFORE" = "$AFTER" ] || { VERDICT=FAIL; note "the slot fact changed during a replay"; }
finish_test
