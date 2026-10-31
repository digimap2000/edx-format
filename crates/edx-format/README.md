# edx-format

Reference reader, checker, merger and writer for [EDX](https://github.com/digimap2000/edx-format), the Electronics
Design eXchange format: one JSON document holding a printed circuit board's design, rules, constraints, layout and
report.

```toml
[dependencies]
edx-format = { git = "https://github.com/digimap2000/edx-format" }
```

## Reading and checking

```rust
let bytes = std::fs::read("board.edx.json")?;
let checked = edx_format::check(&bytes);
for d in &checked.diagnostics {
    eprintln!("{d}"); // error EDX-E2001: /design/parts/3/package: no package `SOT23`
}
if let (true, Some(doc)) = (checked.is_ok(), &checked.document) {
    for part in doc.design.iter().flat_map(|d| d.parts.iter().flatten()) {
        println!("{} uses {}", part.reference, part.package);
    }
}
```

`check` runs the four layers of the specification (parse, version and extensions, schema, semantics) and returns
every diagnostic with its registered code and a JSON Pointer. `read` is the short form: the typed document with its
warnings, or the errors.

## Writing

Build or edit a `Document` (the `model` module mirrors the schema) and write it:

```rust
let text = edx_format::to_string(&doc);            // readable, as the document was arranged
let text = edx_format::to_canonical_string(&doc);  // canonical form: one spelling per meaning
```

Every object keeps its `extensions` and `extras` in its `ext` field, so reading and writing a document loses nothing
this version of EDX defines.

## Merging and hashing

```rust
let merged = edx_format::merge(&design_json, &rules_json); // by record identity; conflicts are EDX-E2007
let hashes = edx_format::hashes(&doc);                     // sha256 of design, rules and constraints
```

A layout engine records `hashes` in its `layout.inputs` and `report.inputs`; a reader warns (`EDX-W2013`) when they
no longer match.

## Geometry and lookups

```rust
use edx_format::{geometry, shape};

let design = doc.design.as_ref().unwrap();
let package = design.package_of("U1").unwrap();
let place = doc.transform("U1").unwrap();          // layout placement, else the design's fixed one
for pad in &package.pads {
    let copper = place.apply_subpaths(&geometry::pad_shape(pad)?); // the pad's copper on the board
    let layers = place.layers(pad.layers);                            // top and bottom swap on the bottom
}
let d = shape::polygon(&[[0.0, 0.0], [10.0, 0.0], [10.0, 5.0]], &[]); // points to a shape
let supply = doc.rules.as_ref().unwrap().class("Supply", Some("bottom")); // with default's values
```

`Transform` maps package coordinates to the board and back (`invert`, `invert_shape`), mirroring the bottom side
as section 2.3 says. `pad_shape` gives any pad's copper as a shape; `shape::write`, `shape::ring` and
`shape::polygon` write shapes from points.

## Constraints in force

```rust
let all = edx_format::derive::constraints(&doc); // derived from relations and rules, authored ones winning
```

`derive::derived` gives only those derived from `design.relations` and `rules.relations` (section 9.5); its module
documentation lists which rule key derives which constraint.

## Licence

MIT OR Apache-2.0, at your option.
