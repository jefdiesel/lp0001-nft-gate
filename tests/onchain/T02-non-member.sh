#!/bin/bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# T2: an account that holds nothing (h2: initialised holding, owns nothing).
# Pass: refused with NOT_A_MEMBER, both by the wallet's own-note check and, with that check
# skipped, by the gate in simulation; nothing sent; h2 has no slot.
. "$(dirname "$0")/env.sh"; . "$(dirname "$0")/lib.sh" T02
run local $CLI prove --holder h2 --payer h2 --caller-identity t2 --purpose T2 --app-nonce $(nonce32)
expect_refused local NOT_A_MEMBER
run sim $CLI prove --holder h2 --payer h2 --caller-identity t2 --purpose T2 --app-nonce $(nonce32) --test-skip-local-member-check
expect_refused sim "gate: NOT_A_MEMBER"
expect_no_text sim "submitted "
run slot $CLI slot --holder h2
expect_text slot "no slot yet"
finish_test
