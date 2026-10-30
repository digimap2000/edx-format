# Conformance suite

Language-neutral test cases that every EDX implementation runs.

| Folder | Contents | Status |
| --- | --- | --- |
| `examples/` | complete synthetic documents, also used by the documentation | started |
| `valid/` | documents that must be read without error, each naming the clause it exercises | to come |
| `invalid/` | one fault per case, with the error code and JSON Pointer a conforming reader must report | to come |
| `canonical/` | an input and its exact canonical output and section hashes | to come |

The examples:

- `two-layer.edx.json`: a two-layer board with rounded corners; an IC with its decoupling and bulk capacitors, a
  fixed connector, a keep-out region and a ground pour; derived and authored constraints.
- `four-layer-island.edx.json`: four layers with ground and supply planes and a `+1V8` island on the supply plane; a
  crystal with load capacitors, a filtered core rail, a part-owned region and a requested block.
- `rules-only.edx.json`: a document holding only rules, to be merged onto a design that has none.

Every example is checked against the schema on each pull request.

Licence: CC0 1.0 (see `LICENSE-CC0` in this folder).
