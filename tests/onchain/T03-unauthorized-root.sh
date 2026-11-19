#!/bin/bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# T3 (run FIRST): the root is not authorised: the holder takes part with an nsk-only witness.
# Pass: refused with the gate's own text "gate: root not authorized", and the CONTROL (the
# same nsk-only shape against the TEST ONLY unsafe gate, which has no such assert) applies.
# If both refuse with another text, LEZ refused before the gate ran: recorded as PASS-BY-LEZ.
# If T3 fails: STOP (spec §8.1).
. "$(dirname "$0")/env.sh"; . "$(dirname "$0")/lib.sh" T03
N=$(nonce32)
run main $CLI prove --holder h1 --payer gas --caller-identity t3 --purpose T3 --app-nonce $N --test-unauthorized-root
run control $CLI prove --holder h1 --payer gas --caller-identity t3 --purpose T3c --app-nonce $N --test-unauthorized-root --test-unsafe-gate
if has control UNEXPECTED_OUTCOME; then
  # The client's outcome rule refused the control, not LEZ: a client-rule verdict, not a design
  # failure. Rerun the control with the rule off to see what LEZ does, and fix the rule.
  note "CLIENT-RULE: outcome check refused the control: $(grep -m1 UNEXPECTED_OUTCOME $O/control.out)"
  run control $CLI prove --holder h1 --payer gas --caller-identity t3 --purpose T3c2 --app-nonce $N --test-unauthorized-root --test-unsafe-gate --test-skip-outcome-check
fi
if has main "gate: root not authorized"; then
  expect_refused main "gate: root not authorized"
  expect_ok control
  has control "applied in block" && note "control applied: the refusal is the gate's assert"
elif [ "$(cat $O/main.rc)" != 0 ] && [ "$(cat $O/control.rc)" != 0 ]; then
  VERDICT=PASS-BY-LEZ; note "both refused before the gate's assert: $(grep -m1 -iE 'refused|error|panicked' $O/main.out)"
else
  VERDICT=FAIL; note "main was not refused by the gate"
fi
run slot $CLI slot --holder h1
finish_test
