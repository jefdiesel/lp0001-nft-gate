#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""T11: which 32-byte windows two transactions share (the method of F A2.3).

linkability.py same1.json same2.json other.json allow=hex,...
- same1, same2: two Prove transactions from ONE holding; other: one from ANOTHER holding.
- baseline = windows shared by (same1, other) and by (same2, other): what every Prove shares.
- extra = windows shared by (same1, same2) beyond the baseline. Every extra window must
  overlap an occurrence of an allowed value (the slot account, the slot, the gate): the slot is
  the one value allowed to repeat for one holding (limit L4).
Prints PASS or FAIL and the runs that are not explained."""
import base64, json, sys

def load(p):
    return base64.b64decode(json.load(open(p))["result"][0])

def windows(b):
    return {b[i:i + 32]: i for i in range(len(b) - 31)}

a, b, c = (load(p) for p in sys.argv[1:4])
allow = [bytes.fromhex(x) for a_ in sys.argv[4:] if a_.startswith("allow=") for x in a_[6:].split(",") if x]
wa, wb, wc = windows(a), windows(b), windows(c)
base = (set(wa) & set(wc)) | (set(wb) & set(wc))
extra = (set(wa) & set(wb)) - base
covered = set()
for v in allow:
    i = a.find(v)
    while i >= 0:
        covered.update(range(i - 31, i + len(v)))
        i = a.find(v, i + 1)
unexplained = sorted(wa[w] for w in extra if wa[w] not in covered)
print(f"sizes {len(a)} {len(b)} {len(c)}; shared same-holder {len(set(wa) & set(wb))}, "
      f"baseline {len(base)}, extra {len(extra)}, unexplained {len(unexplained)}")
runs, start, prev = [], None, None
for o in unexplained:
    if start is None or o != prev + 1:
        if start is not None:
            runs.append((start, prev))
        start = o
    prev = o
if start is not None:
    runs.append((start, prev))
for s, e in runs:
    print(f"  unexplained run at {s}..{e + 31}: {a[s:e + 32].hex()[:96]}")
print("PASS" if not unexplained else "FAIL")
