#!/bin/bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# BUILD HOST, our public rc3 chain only (run through rc3/with-rc3.sh, after 02-setup-rc3.sh and
# the deploys). End to end on a chain that verifies real receipts, with ONE real proof:
#  1. h1 proves membership for actA's next Act at the reference consumer: 91-real-proof.sh
#     (in-process prover, 4 threads, footprint sampler). The caller runs one proof at a time; there
#     is no lock wrapper. Its time.log and sampler.log are what BENCH-proof-and-gas.sh reads (row 31).
#  2. T08's first half, same assertions: actB's Act refused BINDING_MISMATCH; actA's Act applies
#     once (nonce N -> N+1); its Gated outcome; the Act's gas (row 32: declared limit and what the
#     payer was actually charged); verify --tx-hash re-checks the use (getEvents or, on a
#     standalone sequencer, the Act transaction); a second Act refused; an Act on an empty slot
#     refused.
#  3. T01's privacy search on the REAL Prove transaction: holder, collection and slot secret
#     must not appear in its stored bytes.
# Not here (each needs another real proof): T08's late Act, T03/T04/T06 (TEST ONLY programs or
# extra proofs), T05, T07, T09-T13. Log: $G/run/E2E/log.txt.
set -uo pipefail
D=$(cd "$(dirname "$0")/.." && pwd)
. "$D/env.sh"; . "$D/lib.sh" E2E; . "$D/rc3/prove-lib.sh"
[ "${LP0001_REAL_PROVING:-0}" = 1 ] || { echo "STOP: run through rc3/with-rc3.sh"; exit 1; }
cfgv() { python3 -I -c "import json,sys;print(json.load(open('$GATE_CONFIG')).get(sys.argv[1]) or '')" "$1"; }
[ -n "$(cfgv consumer)" ] || { echo "STOP: no consumer in $GATE_CONFIG"; exit 1; }
bal() { $CLI wallet account get --account-id "Public/$1" 2>/dev/null | grep -oE "^Balance [0-9]+" | awk '{print $2}'; }
ACTA=$(acct actA)
# 1. the one real proof (on-chain context for actA)
if [ -n "${E2E_REUSE_PROOF:-}" ]; then
  P=$E2E_REUSE_PROOF; note "reusing the proof in $P (no new proof)"
else
  ollama_clear || { VERDICT=FAIL; note "an Ollama model stayed loaded: not proving"; finish_test; exit 1; }
  note "$(date -u +%H:%M:%S) real proof start; free $(memory_pressure -Q | awk -F': ' '/percentage/{print $2}')"
  run real-proof env PROVE_HOLDER=h1 PROVE_ONCHAIN_ACTOR=actA "$R/tests/onchain/91-real-proof.sh"
  P=$(ls -1dt "$G"/run/real-proof-* 2>/dev/null | head -1)
fi
cp "$P/client.log" "$O/prove.out"; grep -q "^proof {" "$O/prove.out"; echo $? > "$O/prove.rc"
expect_ok prove
expect_text prove "check 6 outcome: passed"
expect_text prove "read back: OK"
note "proof dir $P; $(grep -E ' real ' "$P/time.log" | awk '{print "wall " $1 " s"}'); peak $(grep footprint_kb "$P/sampler.log" | sort -t= -k2 -n | tail -1 | grep -oE 'footprint_kb=[0-9]+')"
cp "$P/proof.json" "$O/proof.json" 2>/dev/null
SLOT=$(python3 -I -c "import json;print(json.load(open('$O/proof.json'))['slot'])")
PTX=$(tx_of prove)
note "Prove tx $PTX block $(python3 -I -c "import json;print(json.load(open('$O/proof.json'))['block'])")"
# 3. privacy search on the real Prove transaction
curl -s -X POST $RPC -H content-type:application/json -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"getTransaction\",\"params\":[\"$PTX\"]}" > $O/raw.json
run slot $CLI slot --holder h1 --test-show-secret
SECRET=$(awk '/^TEST ONLY secret/{print $4}' $O/slot.out)
python3 -I "$R/tools/search_tx.py" $O/raw.json holder=$(b58hex $(acct h1)) payer=$(b58hex $(acct h1)) \
  collection=$(b58hex $(acct defn)) secret=$SECRET > $O/search.txt
cat $O/search.txt >> "$LOG"
for k in holder payer collection secret; do grep -q "^$k: absent" $O/search.txt || { VERDICT=FAIL; note "$k is in the clear in the Prove tx"; }; done
run decode-prove $CLI decode-tx --hash $PTX
# 2. the on-chain use (public transactions: no proof)
run act-B $CLI act --actor actB --slot $SLOT
expect_refused act-B "consumer: BINDING_MISMATCH"
A0=$(bal $ACTA)
run act-A $CLI act --actor actA --slot $SLOT
expect_ok act-A
A1=$(bal $ACTA)
N0=$(grep -oE "^Act by [^ ]+ at consumer nonce [0-9]+" $O/act-A.out | grep -oE "[0-9]+$")
if [ -z "$N0" ]; then VERDICT=FAIL; note "act-A printed no nonce before the Act";
else expect_text act-A "consumer nonce $N0 -> $((N0 + 1));"; note "A's consumer nonce before the Act: $N0"; fi
ATX=$(tx_of act-A)
run gated $CLI gated --tx $ATX
expect_text gated "reverted false"
run decode-act $CLI decode-tx --hash $ATX
DECL=$(grep '^fee ' $O/decode-act.out | grep -oE 'gas_limit: [0-9]+, tip: [0-9]+, max_fee: [0-9]+')
note "Act tx $ATX $(grep -m1 -oE 'applied in block [0-9]+' $O/act-A.out)"
note "Act gas: fee $(( ${A0:-0} - ${A1:-0} )) charged to actA (balance $A0 -> $A1); declared $DECL"
run both-ways $CLI verify --slot $SLOT --tx-hash $ATX
expect_text both-ways "outcome OK"
has both-ways "getEvents unavailable" && note "both-ways: no getEvents on $RPC; verified from the Act transaction and consumer state"
run act-A2 $CLI act --actor actA --slot $SLOT
expect_refused act-A2 "consumer: BINDING_MISMATCH"
run act-empty $CLI act --actor actA --slot $(nonce32)
[ "$(cat $O/act-empty.rc)" != 0 ] || { VERDICT=FAIL; note "Act on an empty slot applied"; }
note "empty slot: $(grep -m1 -oE 'consumer: [A-Z_]+|not admitted|neither a declared[^,]*' $O/act-empty.out)"
run read-slot $CLI read-slot --slot $SLOT
note "not run here (each needs another real proof): T08 late Act, T03-T07, T09-T13"
finish_test
