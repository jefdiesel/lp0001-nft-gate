# SPDX-License-Identifier: MIT OR Apache-2.0
# Helpers for the on-chain tests. Usage in a test:  . "$(dirname "$0")/env.sh"; . "$(dirname "$0")/lib.sh" T03
TNAME=$1
O=$G/run/$TNAME
mkdir -p "$O"
LOG=$O/log.txt
: > "$LOG"
RESULTS=$G/run/RESULTS.txt
VERDICT=PASS
NOTES=""
note() { echo "# $*" | tee -a "$LOG"; NOTES="$NOTES; $*"; }
# run NAME cmd...  -> output in $O/NAME.out, exit code in $O/NAME.rc; returns the exit code
run() {
  local name=$1; shift
  echo "\$ $*" > "$O/$name.out"
  "$@" >> "$O/$name.out" 2>&1
  local rc=$?
  echo "$rc" > "$O/$name.rc"
  echo "[$name exit $rc] $*" >> "$LOG"
  return $rc
}
out() { cat "$O/$1.out"; }
has() { grep -q -- "$2" "$O/$1.out"; }
# expect_ok NAME            the step must have exited 0
expect_ok() { [ "$(cat "$O/$1.rc")" = 0 ] || { VERDICT=FAIL; note "$1 should have succeeded (exit $(cat "$O/$1.rc"))"; }; }
# expect_refused NAME TEXT  the step must have failed and its output must contain TEXT
expect_refused() {
  if [ "$(cat "$O/$1.rc")" = 0 ]; then VERDICT=FAIL; note "$1 should have been refused";
  elif ! has "$1" "$2"; then VERDICT=FAIL; note "$1 refused, but without '$2'"; fi
}
# blocked WHY: a check that cannot run on this chain for a stated reason. The test does not
# pass (finish_test returns 1) but says why, instead of failing on a symptom.
blocked() { [ "$VERDICT" = FAIL ] || VERDICT=BLOCKED; note "BLOCKED: $*"; }
# events_served: the RPC answers getEvents (the LEZ indexer's method) at all
events_served() { ! curl -s -X POST $RPC -H content-type:application/json \
  -d '{"jsonrpc":"2.0","id":1,"method":"getEvents","params":[{}]}' | grep -q '"Method not found"'; }
# expect_text NAME TEXT
expect_text() { has "$1" "$2" || { VERDICT=FAIL; note "$1 lacks '$2'"; }; }
# expect_no_text NAME TEXT
expect_no_text() { has "$1" "$2" && { VERDICT=FAIL; note "$1 contains '$2'"; }; true; }
nonce32() { openssl rand -hex 32; }
b58hex() { python3 -I "$R/tools/b58.py" hex "$1"; }
acct() { $CLI wallet account list 2>/dev/null | grep "\[$1\]" | awk '{print $2}' | sed 's|^Public/||; s|^Private/||' | head -1; }
slot_of() { $CLI slot --holder "$1" 2>/dev/null | awk '/^slot /{print $2}'; }
proof_field() { grep -o "^proof {.*" "$O/$1.out" | sed 's/^proof //' | python3 -I -c "import json,sys;print(json.load(sys.stdin)['$2'])"; }
tx_of() { grep -oE "^submitted [0-9a-f]{64}" "$O/$1.out" | tail -1 | cut -d' ' -f2; }
chain_ms() { curl -s -X POST $RPC -H content-type:application/json -d '{"jsonrpc":"2.0","id":1,"method":"getLastBlockId","params":[]}' \
  | python3 -I -c "import json,sys;print(json.load(sys.stdin)['result'])" | xargs -I{} curl -s -X POST $RPC -H content-type:application/json \
  -d '{"jsonrpc":"2.0","id":1,"method":"getBlock","params":[{}]}' | python3 -I -c "
import json,sys,base64,struct;b=base64.b64decode(json.load(sys.stdin)['result']);print(struct.unpack('<Q',b[72:80])[0])"; }
grid() { echo $(( $1 - $1 % 600000 )); }
finish_test() {
  echo "$TNAME $VERDICT$NOTES" | tee -a "$LOG"
  echo "$(date -u +%FT%TZ) $TNAME $VERDICT$NOTES" >> "$RESULTS"
  case $VERDICT in PASS*) return 0 ;; *) return 1 ;; esac
}
