"""Check that the conformance cases agree with the JSON Schema.

A case whose only expected errors are EDX-E1000 must fail the schema. A case expected to
reach the semantic layer (no errors, or only semantic errors) must pass it. Cases that stop
at parsing or at the version check, and documents of a newer minor version, are skipped:
the schema alone does not decide them.

Usage: python tools/schema_cases.py
"""
import json
import pathlib
import sys

import jsonschema

ROOT = pathlib.Path(__file__).resolve().parent.parent
EARLY = {"EDX-E0001", "EDX-E0002", "EDX-E0003", "EDX-E1001", "EDX-E1002", "EDX-E1003", "EDX-E1004"}

schema = json.loads((ROOT / "schema/1.0/edx.schema.json").read_text())
validator = jsonschema.Draft202012Validator(schema)
cases = json.loads((ROOT / "conformance/cases.json").read_text())["cases"]

failures = 0
checked = 0
for case in cases:
    if "merge" in case:
        continue
    codes = {e["code"] for e in case["expect"]["errors"]}
    warnings = {w["code"] for w in case["expect"]["warnings"]}
    if codes & EARLY or "EDX-W1006" in warnings:
        continue
    document = json.loads((ROOT / "conformance" / case["input"]).read_text(encoding="utf-8"))
    errors = list(validator.iter_errors(document))
    want_invalid = codes == {"EDX-E1000"}
    checked += 1
    if want_invalid and not errors:
        print(f"FAIL {case['name']}: expected a schema error, schema accepted it")
        failures += 1
    elif not want_invalid and errors:
        print(f"FAIL {case['name']}: expected to pass the schema, got: {errors[0].message}")
        failures += 1
    elif want_invalid:
        expected = [e["pointer"] for e in case["expect"]["errors"]]
        reported = ["/" + "/".join(str(p) for p in e.absolute_path) if e.absolute_path else "" for e in errors]
        for pointer in expected:
            if not any(r == pointer or r.startswith(pointer + "/") for r in reported):
                print(f"FAIL {case['name']}: no schema error at or below {pointer!r}; got {reported}")
                failures += 1

print(f"{checked} cases checked against the schema, {failures} failures")
sys.exit(1 if failures else 0)
