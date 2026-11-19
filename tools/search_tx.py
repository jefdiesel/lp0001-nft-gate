#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Search an included transaction's stored bytes for named 32-byte values.

search_tx.py <getTransaction-result.json> name=hex [name=hex ...]
Prints one line per name: FOUND (with offsets) or absent. The bytes searched are the borsh
bytes the chain stores (getTransaction returns them base64-encoded)."""
import base64, json, sys

raw = json.load(open(sys.argv[1]))["result"]
tx = base64.b64decode(raw[0])
print(f"tx bytes {len(tx)} block {raw[1]}")
for arg in sys.argv[2:]:
    name, h = arg.split("=", 1)
    if not h:
        print(f"{name}: (empty, skipped)")
        continue
    needle = bytes.fromhex(h)
    offs, i = [], tx.find(needle)
    while i >= 0:
        offs.append(i); i = tx.find(needle, i + 1)
    print(f"{name}: FOUND at {offs}" if offs else f"{name}: absent")
