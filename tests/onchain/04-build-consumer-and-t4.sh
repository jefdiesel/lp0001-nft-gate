#!/bin/bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# BUILD HOST, through the lock (after 03):
#   ssh build-host '$BUILD_ROOT/dev/fix-names/mini-lock.sh $BUILD_ROOT/lp0001/gate/repo/tests/onchain/04-build-consumer-and-t4.sh > $BUILD_ROOT/lp0001/gate/run/build-consumer.log 2>&1'
# The consumer's constants need the deployed gate, the collection and the channel id.
set -euo pipefail
. "$(dirname "$0")/env.sh"
export CARGO_BUILD_JOBS=4 TMPDIR=$G/tmp
command -v r0vm >/dev/null 2>&1 && { echo "STOP: r0vm on PATH"; exit 1; }
PACK=$J/guest-target/debug/risc0-packager
cfg() { python3 -I -c "import json;print(json.load(open('$GATE_CONFIG'))['$1'])"; }
hexid() { python3 -I "$R/tools/b58.py" hex "$1"; }
build() {
  ( cd "$R/$1" && CARGO_TARGET_DIR=$G/guest-target/$3 cargo +risc0 build --release --target riscv32im-risc0-zkvm-elf )
  "$PACK" "$G/guest-target/$3/riscv32im-risc0-zkvm-elf/release/$2" "$G/out/$3.bin"
  echo "$3: $(wc -c < "$G/out/$3.bin") bytes, sha256 $(shasum -a 256 "$G/out/$3.bin" | cut -d' ' -f1), program id $("$SPEL" program-id "$G/out/$3.bin")"
}
export LP0001_GATE=$(hexid "$(cfg gate)") LP0001_NFT_PROGRAM=$(hexid $NFT) LP0001_COLLECTION=$(hexid "$(cfg collection)")
export LP0001_SUBMISSION_ID=$(cfg submission_id) LP0001_ZONE_ID=$(cfg zone_id)
build guests/consumer lp0001-consumer consumer
# TEST ONLY (T4): the attacker stand-in, never deployed beyond this chain.
ATK=$($CLI wallet account list 2>/dev/null | grep "\[atk\]" | awk '{print $2}' | sed 's|^Public/||')
export LP0001_T4_REAL_NFT=$LP0001_NFT_PROGRAM LP0001_T4_ATTACKER=$(hexid "$ATK") LP0001_T4_COLLECTION=$LP0001_COLLECTION
build tests-only/hostile-guest lp0001-t4-hostile t4-hostile
