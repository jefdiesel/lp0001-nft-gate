#!/bin/bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# BUILD HOST, rc3 only (rc3/with-rc3.sh). Finish an E2E-rc3.sh run whose only failure was the
# re-check (`verify --tx-hash`), after a fix to the verifier: NO new proof and NO new Act.
# The run's directory is kept as run/E2E-attempt1; its recorded outputs of the proof and the
# Acts (already on chain) are re-asserted with E2E-rc3.sh's own checks, and the read-only steps
# (gated, decode, verify, read-slot) are re-run live against the same transactions.
set -uo pipefail
D=$(cd "$(dirname "$0")/.." && pwd)
. "$D/env.sh"
[ "${LP0001_REAL_PROVING:-0}" = 1 ] || { echo "STOP: run through rc3/with-rc3.sh"; exit 1; }
A=$G/run/E2E-attempt1
[ -d "$A" ] || mv "$G/run/E2E" "$A"
. "$D/lib.sh" E2E
note "finishing the run recorded in $A (no new proof, no new Act)"
for f in prove act-B act-A act-A2 act-empty slot decode-prove; do cp "$A/$f.out" "$O/$f.out"; cp "$A/$f.rc" "$O/$f.rc"; done
cp "$A/proof.json" "$A/raw.json" "$A/search.txt" "$O/"
while read -r l; do note "recorded: $l"; done < <(grep -E "^# (proof dir|Prove tx|Act tx|Act gas|A's consumer nonce|empty slot)" "$A/log.txt" | sed 's/^# //')
expect_ok prove
expect_text prove "check 6 outcome: passed"
expect_text prove "read back: OK"
cat $O/search.txt >> "$LOG"
for k in holder payer collection secret; do grep -q "^$k: absent" $O/search.txt || { VERDICT=FAIL; note "$k is in the clear in the Prove tx"; }; done
expect_refused act-B "consumer: BINDING_MISMATCH"
expect_ok act-A
N0=$(grep -oE "^Act by [^ ]+ at consumer nonce [0-9]+" $O/act-A.out | grep -oE "[0-9]+$")
expect_text act-A "consumer nonce $N0 -> $((N0 + 1));"
expect_refused act-A2 "consumer: BINDING_MISMATCH"
[ "$(cat $O/act-empty.rc)" != 0 ] || { VERDICT=FAIL; note "Act on an empty slot applied"; }
SLOT=$(python3 -I -c "import json;print(json.load(open('$O/proof.json'))['slot'])")
ATX=$(tx_of act-A)
# live, read-only
run gated $CLI gated --tx $ATX
expect_text gated "reverted false"
run decode-act $CLI decode-tx --hash $ATX
run both-ways $CLI verify --slot $SLOT --tx-hash $ATX
expect_text both-ways "outcome OK"
has both-ways "getEvents unavailable" && note "both-ways: no getEvents on $RPC; verified from the Act transaction and consumer state"
run read-slot $CLI read-slot --slot $SLOT
expect_text read-slot "fact binding"
note "not run here (each needs another real proof): T08 late Act, T03-T07, T09-T13"
finish_test
