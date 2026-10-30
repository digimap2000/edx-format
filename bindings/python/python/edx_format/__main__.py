"""``edx``: check, merge and format EDX documents, as the reference command-line tool does."""

from __future__ import annotations

import argparse
import json
import sys
from typing import List, Optional

import edx_format


def _report(label: str, result: edx_format.CheckResult) -> None:
    for d in result.diagnostics:
        print(f"{label}: {d}")
    errors, warnings = len(result.errors), len(result.warnings)
    print(f"{label}: " + (f"ok ({warnings} warnings)" if not errors else f"{errors} errors, {warnings} warnings"), file=sys.stderr)


def _json(label: str, result: edx_format.CheckResult, merged: Optional[dict] = None) -> None:
    out = {"documents": [{"file": label, "ok": result.ok, "diagnostics": [d.__dict__ for d in result.diagnostics]}]}
    if merged is not None:
        out["result"] = merged
    print(json.dumps(out, ensure_ascii=False))


def _read(path: str) -> bytes:
    if path == "-":
        return sys.stdin.buffer.read()
    with open(path, "rb") as f:
        return f.read()


def main(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(prog="edx", description="Check, merge and format EDX documents.")
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("check", help="check documents; exit status 1 if any has errors")
    p.add_argument("--format", choices=["text", "json"], default="text")
    p.add_argument("files", nargs="+")
    p = sub.add_parser("merge", help="merge two documents and check the result")
    p.add_argument("--format", choices=["text", "json"], default="text")
    p.add_argument("-o", "--output")
    p.add_argument("first")
    p.add_argument("second")
    p = sub.add_parser("fmt", help="write a document in canonical form")
    p.add_argument("-o", "--output")
    p.add_argument("file")
    p = sub.add_parser("hash", help="print the hashes of its design, rules and constraints")
    p.add_argument("--format", choices=["text", "json"], default="text")
    p.add_argument("file")
    sub.add_parser("schema", help="print the JSON Schema")
    args = parser.parse_args(argv)

    if args.command == "check":
        ok = True
        for path in args.files:
            result = edx_format.check(_read(path))
            ok &= result.ok
            (_json if args.format == "json" else _report)(path, result)
        return 0 if ok else 1
    if args.command == "merge":
        first, second = edx_format.check(_read(args.first)), edx_format.check(_read(args.second))
        label = f"{args.first} + {args.second}"
        if not first.ok:
            label, result = args.first, first
        elif not second.ok:
            label, result = args.second, second
        else:
            result = edx_format.merge(first.document, second.document)
        merged = result.document if result.ok else None
        if merged is not None and args.output:
            with open(args.output, "w", encoding="utf-8", newline="\n") as f:
                f.write(edx_format.dumps(merged))
        if args.format == "json":
            _json(label, result, merged)
        else:
            _report(label, result)
            if merged is not None and not args.output:
                sys.stdout.write(edx_format.dumps(merged))
        return 0 if result.ok else 1
    if args.command in ("fmt", "hash"):
        try:
            data = _read(args.file)
            if args.command == "fmt":
                text = edx_format.dumps(data, canonical=True)
                if args.output:
                    with open(args.output, "w", encoding="utf-8", newline="\n") as f:
                        f.write(text)
                else:
                    sys.stdout.buffer.write(text.encode())
            else:
                hashes = edx_format.hashes(data)
                if args.format == "json":
                    print(json.dumps(hashes))
                else:
                    for section, value in hashes.items():
                        print(f"{section:<12} {value}")
        except edx_format.EdxError as e:
            print(f"{args.file}: {e}", file=sys.stderr)
            return 1
        return 0
    sys.stdout.write(edx_format.SCHEMA)
    return 0


if __name__ == "__main__":
    sys.exit(main())
