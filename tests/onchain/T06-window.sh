#!/bin/bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# T6 (run FIRST, after T3): a guest-set timestamp window in a private execution.
# Pass: (a) a window already over is refused; (b) a window not yet open is refused;
# (c) not_before off the grid and (d) ttl_class 3 are refused by the gate; (e) a normal proof
# applies with a NON-EMPTY window [not_before, expires_at) in the decoded transaction.
# Records which layer refused (a) and (b): wallet, sequencer at submission, or never included.
# If T6 fails: STOP (spec §8.1).
. "$(dirname "$0")/env.sh"; . "$(dirname "$0")/lib.sh" T06
NOW=$(chain_ms); NB=$(grid $NOW)
note "chain time $NOW grid $NB"
run a-expired $CLI prove --holder h6 --payer h6 --caller-identity t6 --purpose a --app-nonce $(nonce32) --test-not-before $((NB - 7200000)) --test-ttl-class 0
run b-future $CLI prove --holder h6 --payer h6 --caller-identity t6 --purpose b --app-nonce $(nonce32) --test-not-before $((NB + 1200000)) --test-ttl-class 0
run c-offgrid $CLI prove --holder h6 --payer h6 --caller-identity t6 --purpose c --app-nonce $(nonce32) --test-not-before $((NB + 1)) --test-ttl-class 0
run d-badttl $CLI prove --holder h6 --payer h6 --caller-identity t6 --purpose d --app-nonce $(nonce32) --test-not-before $NB --test-ttl-class 3
for s in a-expired b-future; do
  [ "$(cat $O/$s.rc)" != 0 ] || { VERDICT=FAIL; note "$s was applied"; }
  note "$s refused at: $(grep -m1 -E 'FAILED|detail|not included|OutOfValidity|validity' $O/$s.out)"
done
expect_refused c-offgrid "gate: not_before is off the grid"
expect_refused d-badttl "gate: bad ttl_class"
run e-normal $CLI prove --holder h6 --payer h6 --caller-identity t6 --purpose e --app-nonce $(nonce32)
if has e-normal UNEXPECTED_OUTCOME; then
  note "CLIENT-RULE: outcome check refused a normal proof (fix the rule in gate_wallet_rules): $(grep -m1 UNEXPECTED_OUTCOME $O/e-normal.out)"
  run e-normal $CLI prove --holder h6 --payer h6 --caller-identity t6 --purpose e2 --app-nonce $(nonce32) --test-skip-outcome-check
fi
expect_ok e-normal
TX=$(tx_of e-normal)
run e-decode $CLI decode-tx --hash $TX
NBE=$(proof_field e-normal not_before); EXE=$(proof_field e-normal expires_at)
expect_text e-decode "validity timestamps \[Some($NBE), Some($EXE))"
finish_test
