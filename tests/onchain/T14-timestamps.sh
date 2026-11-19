#!/bin/bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# T14 (read-only): do block timestamps ever go backwards? Spec §2.7 marks it UNVERIFIED.
# READ: the sequencer stamps each block with its wall clock, chrono::Utc::now()
# (lez/sequencer/core/src/lib.rs:2053-2054), with no max(previous) guard; the clock program
# checks only that the block id advances by one (lez/programs/clock/src/main.rs:24-29). So the
# source does not prevent a step back (an NTP correction); this scan records what happened on
# the 3310 chain. Pass: no block older than its predecessor. Either way the count is recorded.
. "$(dirname "$0")/env.sh"; . "$(dirname "$0")/lib.sh" T14
run scan python3 -I "$R/tools/timestamps.py" $RPC
expect_text scan "backwards 0"
finish_test
