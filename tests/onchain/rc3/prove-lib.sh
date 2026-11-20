# SPDX-License-Identifier: MIT OR Apache-2.0
# Sourced after env.sh and lib.sh by the rc3 scripts. A private transaction on the rc3 chain is
# a real proof: run it through mini-lock.sh (one heavy job on the Mini at a time, >= 30% free)
# and guard.sh (pause < 15% free, kill < 8% or above 11 GB footprint), never alongside Ollama.
# ollama_clear: wait (up to 12 min) for no model loaded; never pauses or stops Ollama itself.
ollama_clear() {
  local i
  for i in $(seq 1 72); do
    curl -s -m 5 http://127.0.0.1:11434/api/ps | grep -q '"models":\[\]' && return 0
    [ "$i" = 1 ] && echo "# waiting for Ollama to unload its model before proving" | tee -a "$LOG"
    sleep 10
  done
  return 1
}
# prove_run NAME cmd...: like run, through the lock and the guard; guard log in $O/NAME.guard.log
prove_run() {
  local name=$1; shift
  if ! ollama_clear; then
    echo 1 > "$O/$name.rc"; echo "STOP: an Ollama model stayed loaded" > "$O/$name.out"
    note "$name not started: an Ollama model stayed loaded for 12 min (not paused by this script)"; return 1
  fi
  echo "# $(date -u +%H:%M:%S) proving $name; free $(memory_pressure -Q | awk -F': ' '/percentage/{print $2}')" | tee -a "$LOG"
  run "$name" "$LP0001_MINILOCK" env GUARD_LOG="$O/$name.guard.log" MAX_FOOTPRINT_GB=11 "$LP0001_GUARD" "$@"
  local rc=$?
  grep -h "guard: exit" "$O/$name.out" | tail -1 | sed "s/^/# $name /" | tee -a "$LOG"
  return $rc
}
