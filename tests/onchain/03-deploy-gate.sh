#!/bin/bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# BUILD HOST. Deploys the gate with an IMMUTABLE header (spec §2.1) and the TEST ONLY unsafe
# gate (mutable), reads the zone's channel id, and writes the first half of gate-config.json.
set -uo pipefail
. "$(dirname "$0")/env.sh"
. "$(dirname "$0")/lib.sh" DEPLOY-GATE
PAY=$(acct pay)
run deploy-gate "$SPEL" deploy "$G/out/gate.bin" --fee-payer "$PAY" --immutable
GATE=$(tail -1 "$O/deploy-gate.out" | tr -d '[:space:]')
TGATE=""
if [ "${LP0001_NO_TEST_PROGRAMS:-0}" != 1 ]; then
  run deploy-unsafe "$SPEL" deploy "$G/out/gate-unsafe-test.bin" --fee-payer "$PAY"
  TGATE=$(tail -1 "$O/deploy-unsafe.out" | tr -d '[:space:]')
fi
run channel $CLI channel-id
ZONE=$(tail -1 "$O/channel.out" | tr -d '[:space:]')
run header $CLI header --program "$GATE"
expect_text header "immutable true"
IMAGE=$(awk '/^image_id/{print $2}' "$O/header.out")
EXPECT_IMAGE=$("$SPEL" program-id "$G/out/gate.bin" | tr -d '[:space:]')
[ "$IMAGE" = "$EXPECT_IMAGE" ] || note "header image $IMAGE differs from spel program-id $EXPECT_IMAGE (byte order?) - check before trusting check 4"
SUB=$(python3 -I -c "import hashlib,sys;print(hashlib.sha256(sys.argv[1].encode()).hexdigest())" "${LP0001_SUBMISSION_LABEL:-lp0001-gate/test-submission/throwaway-rc3}")
python3 -I - "$GATE_CONFIG" <<PY
import json,sys
json.dump({"sequencer":"$RPC","gate":"$GATE","gate_image_id":"$IMAGE","nft_program":"$NFT",
 "zone_id":"$ZONE","submission_id":"$SUB","collection":"$(acct defn)","unsafe_test_gate":"$TGATE",
 "max_fee":"6000000","note":"${LP0001_DEPLOY_NOTE:-TEST deployment on the throwaway chain; submission_id is a test value}"},open(sys.argv[1],"w"),indent=1)
PY
cat "$GATE_CONFIG" | tee -a "$LOG"
finish_test
