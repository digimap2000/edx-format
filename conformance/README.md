# Conformance suite

Language-neutral test cases that every EDX implementation runs. `cases.json` lists them all.

| Folder | Contents |
| --- | --- |
| `examples/` | complete synthetic documents, also used by the documentation |
| `valid/` | documents a conforming reader accepts, some with expected warnings |
| `invalid/` | one fault per document, named after the code a conforming reader must report |

## The case list

Each entry in `cases.json` is one case:

```json
{
  "name": "unknown-package",
  "clause": "5.3",
  "description": "…",
  "input": "invalid/E2001-unknown-package.edx.json",
  "expect": {
    "errors": [{ "code": "EDX-E2001", "pointer": "/design/parts/1/package" }],
    "warnings": []
  }
}
```

A merge case has `"merge": [first, second]` instead of `input`: the two documents are merged (section 3.2 of the
specification) and the merge result is checked.

## Passing a case

An implementation passes a case when:

- the errors it reports, as pairs of code and pointer, are exactly the expected errors: no more, no fewer;
- for `EDX-E1000` (a schema error), each expected pointer has a reported `EDX-E1000` at that pointer or below it,
  since validators differ in how deep they point;
- it reports every expected warning (further warnings are allowed);
- for a merge case, merging in either order gives the same result.

`invalid/` files with codes in `EDX-E0xxx` are deliberately broken bytes, not valid JSON. Read them as bytes.

## Checks run in CI

- `tools/schema_cases.py` checks that the cases agree with the JSON Schema: every case expected to fail on the schema
  does, and every case expected to reach the semantic checks passes it.
- The reference implementation runs the whole suite (to come).

Licence: CC0 1.0 (see `LICENSE-CC0` in this folder).
