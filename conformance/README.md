# Conformance suite

Language-neutral test cases that every EDX implementation runs:

- `valid/`: bundles that must be read without error
- `invalid/`: one fault per case, with the error code and JSON Pointer a conforming reader must report
- `canonical/`: an input and its exact canonical output and hash
- `examples/`: complete synthetic boards, also used by the documentation

Licence: CC0 1.0 (see `LICENSE-CC0` in this folder).
