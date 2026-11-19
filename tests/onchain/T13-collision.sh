#!/bin/bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# T13: a send from the holder while a proof is pending: both spend the holder's current
# nullifier, so one of the two fails ("Nullifier already seen", or never included).
# Two copies of the throwaway wallet home, started together (one wallet home must never be
# used twice at once). Pass: exactly one of the two applied.
. "$(dirname "$0")/env.sh"; . "$(dirname "$0")/lib.sh" T13
rm -rf $G/wallet-t13 && cp -R $G/wallet $G/wallet-t13
( $CLI prove --holder h1 --payer h1 --caller-identity t13 --purpose T13 --app-nonce $(nonce32) > $O/prove.out 2>&1; echo $? > $O/prove.rc ) &
( LEE_WALLET_HOME_DIR=$G/wallet-t13 $CLI wallet auth-transfer send --from h1 --to "Public/$(acct pay)" --amount 1 > $O/send.out 2>&1; echo $? > $O/send.rc ) &
wait
A=$(grep -c "applied in block\|Transaction executed\|TransactionExecuted" $O/prove.out $O/send.out | awk -F: '{s+=$2} END {print s}')
note "prove exit $(cat $O/prove.rc), send exit $(cat $O/send.rc)"
note "refusal: $(grep -h -m1 -iE 'nullifier|not included|FAILED' $O/prove.out $O/send.out | head -1)"
[ "$(cat $O/prove.rc)" = 0 ] && [ "$(cat $O/send.rc)" = 0 ] && { VERDICT=FAIL; note "both applied"; }
[ "$(cat $O/prove.rc)" != 0 ] && [ "$(cat $O/send.rc)" != 0 ] && { VERDICT=FAIL; note "neither applied (race lost both ways?): rerun"; }
rm -rf $G/wallet-t13
# the main wallet follows whatever landed
$CLI wallet account sync-private > /dev/null 2>&1 || true
finish_test
