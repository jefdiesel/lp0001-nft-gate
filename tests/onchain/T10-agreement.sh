#!/bin/bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# T10 (after T8): the consumer on chain and the library agree on the same slot bytes.
# Native half: consumer_program's t10 test and both verifiers run the shared vector file (T00).
# Chain half, from T8's runs: OK (Act applied / verify from its event), BINDING_MISMATCH (B's
# Act refused on chain), NO_FACT (empty slot / no event), EXPIRED (sequencer refused, late Act).
. "$(dirname "$0")/env.sh"; . "$(dirname "$0")/lib.sh" T10
T8=$G/run/T08
# Since gate-cli verify reads the actor and nonce from the Act's Gated event (--tx-hash; it no
# longer takes them from the caller), the library can be asked on chain only about an Act that
# applied. B's Act, the empty-slot Act and the late Act are refused and emit no event, so for
# those three codes the library half is the native one (T00: both verifiers and the consumer
# run the shared vector file) and this test checks the chain half hard.
# OK: A's Act applied (nonce 0 -> 1), and the library re-checks it from its event.
grep -q "consumer nonce 0 -> 1" $T8/act-A.out || { VERDICT=FAIL; note "OK pair: A's Act did not apply"; }
if grep -q "outcome OK" $T8/both-ways.out; then :
elif grep -q "BLOCKED: both-ways" $T8/log.txt; then blocked "OK pair, library half: T8's verify --tx-hash could not read events on this chain (see T08)"
else VERDICT=FAIL; note "OK pair: the library did not say OK for A's Act"; fi
# BINDING_MISMATCH: B's Act on A's fact, refused by the consumer with that code.
grep -q "consumer: BINDING_MISMATCH" $T8/act-B.out && [ "$(cat $T8/act-B.rc)" != 0 ] || { VERDICT=FAIL; note "BINDING_MISMATCH pair: the consumer did not refuse B's Act with BINDING_MISMATCH"; }
# NO_FACT: an Act on a never-written slot, refused (by the consumer, or by LEZ first: recorded).
[ "$(cat $T8/act-empty.rc)" != 0 ] || { VERDICT=FAIL; note "NO_FACT pair: the empty-slot Act applied"; }
note "empty slot: $(grep -m1 -oE 'consumer: [A-Z_]+|not admitted|neither a declared[^,]*' $T8/act-empty.out)"
# EXPIRED: a real late fact existed and the Act after its expires_at was refused.
if [ -s $T8/late.json ] && [ -e $T8/act-late.rc ]; then
  [ "$(cat $T8/act-late.rc)" != 0 ] || { VERDICT=FAIL; note "EXPIRED pair: the late Act applied"; }
  grep -q "chain time never reached" $T8/log.txt && { VERDICT=FAIL; note "EXPIRED pair: T8's late Act was not late"; }
else
  VERDICT=FAIL; note "EXPIRED pair missing: T8 made no late fact or tried no late Act"
fi
finish_test
