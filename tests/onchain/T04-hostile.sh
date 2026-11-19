#!/bin/bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# T4 (run LAST): a hostile NFT-shaped program (tests-only/hostile) that sends Transfer to the
# holder's real NFT actor with the holder's authority and answers "owned", called from the TEST
# ONLY unsafe gate that lets the caller name the program. Records whether LEZ admits it (the
# simulation passing), then confirms the client's outcome check stops it before sending.
# Pass: (a) with the attacker's holding declared, simulation passes (LEZ admits it) and the
# outcome check refuses: UNEXPECTED_OUTCOME, "the holder's NFT actor changed", nothing sent,
# h4 still owns its copy; (b) without declaring it, LEZ refuses the undeclared delivery.
# Optional (owner's call, T04_SEND_WITHOUT_CHECK=1): send it with the check OFF, to see the
# sequencer accept it and h4's copy move to atk on this throwaway chain.
. "$(dirname "$0")/env.sh"; . "$(dirname "$0")/lib.sh" T04
HOSTILE=$(python3 -I -c "import json;print(json.load(open('$GATE_CONFIG'))['t4_hostile'])")
run a $CLI prove --holder h4 --payer h4 --test-unsafe-gate --test-nft-program $HOSTILE --test-declare atk --caller-identity t4 --purpose T4 --app-nonce $(nonce32)
expect_text a "check 5 simulation: passed"
expect_refused a "the holder's NFT actor changed"
expect_no_text a "applied in block"
note "LEZ admits it in simulation: $(grep -c 'check 5 simulation: passed' $O/a.out)"
run b $CLI prove --holder h4 --payer h4 --test-unsafe-gate --test-nft-program $HOSTILE --caller-identity t4 --purpose T4b --app-nonce $(nonce32)
[ "$(cat $O/b.rc)" != 0 ] || { VERDICT=FAIL; note "b applied"; }
note "b refused at: $(grep -m1 -E 'refused|neither a declared|FAILED' $O/b.out)"
run read $CLI nft read --account h4
expect_text read "owned: true"
if [ "${T04_SEND_WITHOUT_CHECK:-0}" = 1 ]; then
  run c $CLI prove --holder h4 --payer h4 --test-unsafe-gate --test-nft-program $HOSTILE --test-declare atk --caller-identity t4 --purpose T4c --app-nonce $(nonce32) --test-skip-outcome-check
  run read-after $CLI nft read --account h4
  run read-atk $CLI nft read --account atk
  note "c (check OFF): exit $(cat $O/c.rc); h4 after: $(grep -o 'owned: [a-z]*' $O/read-after.out); atk: $(grep -o 'owned: [a-z]*' $O/read-atk.out)"
fi
finish_test
