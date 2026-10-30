import json
from pathlib import Path

import pytest

import edx_format

SUITE = Path(__file__).resolve().parents[3] / "conformance"


def test_minimal_document():
    result = edx_format.check('{"edx": "1.0"}')
    assert result.ok and result.document == {"edx": "1.0"} and result.diagnostics == []


def test_errors_carry_code_and_pointer():
    result = edx_format.check_file(SUITE / "invalid/E2001-unknown-package.edx.json")
    assert not result.ok
    assert [(d.code, d.pointer) for d in result.errors] == [("EDX-E2001", "/design/parts/1/package")]
    with pytest.raises(edx_format.EdxError) as e:
        edx_format.load(SUITE / "invalid/E2001-unknown-package.edx.json")
    assert e.value.diagnostics[0].code == "EDX-E2001"


def test_parse_errors_have_a_line():
    d = edx_format.check(b"{\n  \"edx\": \"1.0\",\n").diagnostics[0]
    assert d.code == "EDX-E0001" and d.line == 3


def test_dict_round_trip_and_plain_numbers():
    doc = edx_format.load(SUITE / "valid/base.edx.json")
    doc["design"]["packages"][0]["pads"][0]["position"] = [-0.00005, 0]
    text = edx_format.dumps(doc)
    assert "-0.00005" in text and "e-" not in text
    assert edx_format.loads(text) == json.loads(text)


def test_canonical_form_and_hashes_match_the_suite():
    cases = {c["name"]: c for c in json.loads((SUITE / "cases.json").read_text())["cases"]}
    case = cases["canonical-same-meaning"]
    data = (SUITE / case["input"]).read_bytes()
    assert edx_format.dumps(data, canonical=True) == (SUITE / case["expect"]["canonical"]).read_text()
    assert edx_format.hashes(data) == case["expect"]["hashes"]
    assert edx_format.hashes(edx_format.loads(data)) == case["expect"]["hashes"]


def test_merge_in_either_order():
    a = edx_format.load(SUITE / "valid/design-only.edx.json")
    b = edx_format.load(SUITE / "examples/rules-only.edx.json")
    ab, ba = edx_format.merge(a, b), edx_format.merge(b, a)
    assert ab.ok and ab.document == ba.document
    assert set(ab.document) >= {"design", "rules"}


def test_merge_conflict():
    a = edx_format.load(SUITE / "valid/base.edx.json")
    b = edx_format.load(SUITE / "examples/rules-only.edx.json")
    result = edx_format.merge(a, b)
    assert [(d.code, d.pointer) for d in result.errors] == [("EDX-E2007", "/rules/relations/decouples")]


def test_newer_minor_has_no_canonical_form():
    with pytest.raises(edx_format.EdxError):
        edx_format.hashes((SUITE / "valid/newer-minor.edx.json").read_bytes())


def test_tables():
    assert edx_format.CODES["EDX-E3005"].startswith("two pours")
    assert json.loads(edx_format.SCHEMA)["$id"].endswith("/schema/1.0/edx.schema.json")
    assert edx_format.EDX_VERSION == "1.0"
