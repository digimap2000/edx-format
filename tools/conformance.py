#!/usr/bin/env python3
"""Run the EDX conformance suite against any implementation's command-line tool.

The tool must offer, like the reference `edx` command:

    TOOL check --format json FILE          -> {"documents": [{"diagnostics": [...]}]}
    TOOL merge --format json FIRST SECOND  -> the same, plus "result" when the merge succeeds
    TOOL fmt FILE                          -> the document in canonical form
    TOOL hash --format json FILE           -> {"design": "sha256:…", "rules": …, "constraints": …}

Each diagnostic has at least "code", "severity" and "pointer". The exit status is ignored.

Usage: python tools/conformance.py [--verbose] TOOL [ARGS...]
For example: python tools/conformance.py target/debug/edx
             python tools/conformance.py node bindings/js/bin/edx.cjs
"""

import json
import subprocess
import sys
from pathlib import Path

SUITE = Path(__file__).resolve().parent.parent / "conformance"


def run(tool, *args):
    done = subprocess.run([*tool, *args], cwd=SUITE, capture_output=True, text=True)
    try:
        report = json.loads(done.stdout)
        return report["documents"][0]["diagnostics"], report.get("result")
    except (ValueError, KeyError, IndexError):
        raise SystemExit(f"{' '.join(tool)} {' '.join(args)}: unreadable output\n{done.stdout}{done.stderr}")


def output(tool, *args):
    return subprocess.run([*tool, *args], cwd=SUITE, capture_output=True).stdout


def pairs(diagnostics, severity):
    return {(d["code"], d["pointer"]) for d in diagnostics if d.get("severity", "error") == severity}


def problems(case, tool):
    expected_errors = {(d["code"], d["pointer"]) for d in case["expect"]["errors"]}
    expected_warnings = {(d["code"], d["pointer"]) for d in case["expect"]["warnings"]}
    if "merge" in case:
        first, second = case["merge"]
        outcomes = [run(tool, "merge", "--format", "json", first, second),
                    run(tool, "merge", "--format", "json", second, first)]
        if outcomes[0][1] != outcomes[1][1]:
            yield "merging in the two orders gives different results"
    else:
        outcomes = [run(tool, "check", "--format", "json", case["input"])]
    if "canonical" in case["expect"]:
        if output(tool, "fmt", case["input"]) != (SUITE / case["expect"]["canonical"]).read_bytes():
            yield "the canonical form differs from the expected file"
    if "hashes" in case["expect"]:
        try:
            hashes = json.loads(output(tool, "hash", "--format", "json", case["input"]))
        except ValueError:
            hashes = None
        if hashes != case["expect"]["hashes"]:
            yield f"expected hashes {case['expect']['hashes']}, got {hashes}"
    for diagnostics, _ in outcomes:
        errors = pairs(diagnostics, "error")
        if any(code == "EDX-E1000" for code, _ in expected_errors):
            # Validators differ in how deep they point: at the expected pointer or below it passes.
            ok = all(code == "EDX-E1000" for code, _ in errors) and all(
                any(p == pointer or p.startswith(pointer + "/") for _, p in errors) for _, pointer in expected_errors
            )
        else:
            ok = errors == expected_errors
        if not ok:
            yield f"expected errors {sorted(expected_errors)}, got {sorted(errors)}"
        missing = expected_warnings - pairs(diagnostics, "warning")
        if missing:
            yield f"missing warnings {sorted(missing)}"


def main():
    args = sys.argv[1:]
    verbose = "--verbose" in args
    tool = [a for a in args if a != "--verbose"]
    if not tool:
        raise SystemExit(__doc__)
    # The tool runs from inside the suite, so paths given relative to here must be made absolute.
    tool = [str(Path(a).resolve()) if Path(a).exists() else a for a in tool]
    cases = json.loads((SUITE / "cases.json").read_text())["cases"]
    failed = 0
    for case in cases:
        found = list(problems(case, tool))
        failed += bool(found)
        for p in found:
            print(f"FAIL {case['name']}: {p}")
        if verbose and not found:
            print(f"ok   {case['name']}")
    print(f"{len(cases) - failed} of {len(cases)} cases passed")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
