#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""T14: scan every block header of an LEZ RPC; count timestamps that go backwards.
timestamps.py http://127.0.0.1:3310   (getBlock: base64 borsh, timestamp at bytes 72..80)"""
import base64, json, struct, sys, urllib.request

url = sys.argv[1]
def call(method, params):
    req = urllib.request.Request(url, json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode(),
                                 {"content-type": "application/json", "user-agent": "lez-nft-gate-t14"})
    return json.load(urllib.request.urlopen(req, timeout=30))["result"]

last = call("getLastBlockId", [])
prev, back, worst, gaps = None, 0, 0, []
for i in range(1, last + 1):
    b = call("getBlock", [i])
    if b is None:
        continue
    ts = struct.unpack("<Q", base64.b64decode(b)[72:80])[0]
    if prev is not None:
        d = ts - prev
        if d < 0:
            back += 1; worst = min(worst, d); gaps.append((i, d))
    prev = ts
print(f"blocks 1..{last} backwards {back} worst_ms {worst}")
for i, d in gaps[:20]:
    print(f"  block {i}: {d} ms")
