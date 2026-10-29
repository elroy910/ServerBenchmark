#!/usr/bin/env python3
"""Development-only JSON Schema regression check; no benchmark/runtime dependency."""
import copy
import json
import random
from pathlib import Path

from jsonschema import Draft202012Validator

ROOT = Path(__file__).resolve().parent.parent
schema = json.loads((ROOT / "schemas/benchmark-preview-v2.schema.json").read_text())
Draft202012Validator.check_schema(schema)
validator = Draft202012Validator(schema)
fixture = json.loads(
    (ROOT / "tests/fixtures/results/preview-v2-incomplete.json").read_text()
)
validator.validate(fixture)

# Check the decimal-string grammar against integer arithmetic, including values
# beyond JavaScript's exact range and the precise u64 upper bound.
counter = Draft202012Validator(schema["$defs"]["counter"])
rng = random.Random(0)
values = [0, 1, 2**53 + 1, 2**64 - 1, 2**64, 10**20]
values += [rng.randrange(10**20) for _ in range(10000)]
for value in values:
    if counter.is_valid(str(value)) != (value <= 2**64 - 1):
        raise AssertionError(f"incorrect u64 bound: {value}")
for value in [0, -1, 1.0, None, "", "01", "+1", "-1", "1.0", "1e3", " 1", "1\n"]:
    if counter.is_valid(value):
        raise AssertionError(f"accepted noncanonical counter: {value!r}")
for field, value in [
    ("scores", {}),
    ("document_type", "repeat_series"),
    ("score_status", "available"),
    ("status", "unavailable"),
]:
    invalid = copy.deepcopy(fixture)
    invalid[field] = value
    if validator.is_valid(invalid):
        raise AssertionError(f"accepted invalid preview field: {field}")
print("JSON Schema, preview fixture, score/type constraints and randomized u64 bounds passed.")
