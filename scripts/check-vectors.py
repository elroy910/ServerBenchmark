#!/usr/bin/env python3
"""Independent golden-vector verification; development only, never a runtime dependency."""
import hashlib
import json

MASK = (1 << 64) - 1
phrase = b"ServerBench deterministic CPU corpus. records: alpha beta gamma 0123456789\n"
corpus = bytes(phrase[i % len(phrase)] if i % 97 < 80 else (i * 31 + i // 97) % 256 for i in range(65536))
assert hashlib.sha256(corpus).hexdigest() == "fed880d6c2ee7e7c6bdeaf14b99a18fbaa8172e5574fa00e05d35a69f79a4a61"
x, checksum = 0x123456789ABCDEF0, 0
table = [(i * 0x9E3779B97F4A7C15) & MASK for i in range(256)]
for i in range(32768):
    x = (x * 6364136223846793005 + (i ^ 1442695040888963407)) & MASK
    if x & 8 == 0:
        x = (((x << 13) | (x >> 51)) & MASK) ^ table[x & 255]
    else:
        x = (((x >> 7) | (x << 57)) + table[(x >> 8) & 255]) & MASK
    checksum = (checksum + (x ^ (x >> 23))) & MASK
assert checksum == 4644254777524890854

def fnv(data):
    result = 0xCBF29CE484222325
    for byte in data:
        result = ((result ^ byte) * 0x100000001B3) & MASK
    return result

rows = [dict(id=i, name=f"record-{i:04}-alpha", value=((i * 7919) % 100003) - 50000, active=i % 3 != 0) for i in range(256)]
rows = [dict(id=r["id"], name=r["name"].replace("alpha", "BETA").upper(), value=r["value"] * 3 + 17, active=r["active"]) for r in rows if r["active"]]
rows.sort(key=lambda r: (r["value"], r["id"]))
lookup = sum(fnv(r["name"].encode()) for r in rows) & MASK
assert fnv(json.dumps(rows, separators=(",", ":")).encode()) ^ lookup == 4610356586914610349
print("Independent dataset, integer and data-processing vectors verified.")
