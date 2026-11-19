#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""base58 <-> hex for 32-byte LEZ account ids.  b58.py hex <base58> | b58.py b58 <hex>"""
import sys
A = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
def to_hex(s):
    n = 0
    for c in s.strip().replace("Public/", "").replace("Private/", ""):
        n = n * 58 + A.index(c)
    return n.to_bytes(32, "big").hex()
def to_b58(h):
    b = bytes.fromhex(h.strip().removeprefix("0x"))
    n = int.from_bytes(b, "big"); out = ""
    while n:
        n, r = divmod(n, 58); out = A[r] + out
    return "1" * (len(b) - len(b.lstrip(b"\0"))) + out
if __name__ == "__main__":
    print(to_hex(sys.argv[2]) if sys.argv[1] == "hex" else to_b58(sys.argv[2]))
