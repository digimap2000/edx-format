# EDX, the Electronics Design eXchange

EDX is an open, tool-neutral file format for printed circuit board design data. The design side states what the
board is and what it must achieve: outline, stack-up, packages, parts, nets, regions, pours, design-intent
relations, rules and explicit constraints. A layout engine decides how, and returns placement, copper and a report
in the same terms.

An EDX document is one JSON file (`*.edx.json`) with optional sections: `design`, `rules`, `constraints`, and the
engine's `layout` and `report`. Documents holding different sections, such as a design and a set of rules, merge into
one. Areas are written as SVG path data.

**Status:** pre-release. The specification, [`spec/edx.md`](spec/edx.md), is a working draft of version 1.0; nothing is stable yet.

## What lives here

| Folder | Contents |
| --- | --- |
| `spec/` | The specification, the normative definition of EDX |
| `schema/` | The JSON Schema (2020-12) for an EDX document, normative alongside the spec |
| `conformance/` | Language-neutral test cases and example documents |
| `crates/edx-format/` | Reference library in Rust: check, read, merge and write EDX documents |
| `crates/edx-cli/` | `edx`, the command-line checker and merger |
| `bindings/python/` | The Python package `edx-format`, built on the Rust library |
| `bindings/js/` | The JavaScript package `edx-format`, the Rust library compiled to WebAssembly |
| `site/` | The GitHub Pages site that serves the schema at its `$id` |
| `tools/` | Scripts that check the suite against the schema and run it against any implementation |

## The reference library

The Rust crate `edx-format` checks a document in the four layers of the specification (parse, version and
extensions, schema, semantics), reads it into a typed model that keeps every `extensions` and `extras` member, merges
documents by record identity, and writes them back, plainly or in canonical form, with the section hashes a layout
engine records.

```rust
let checked = edx_format::check(&std::fs::read("board.edx.json")?);
for diagnostic in &checked.diagnostics {
    println!("{diagnostic}"); // error EDX-E2001: /design/parts/3/package: no package `SOT23`
}
if let Some(document) = checked.document.filter(|_| checked.is_ok()) {
    println!("{} parts", document.design.and_then(|d| d.parts).map_or(0, |p| p.len()));
}
```

The `edx` command does the same from a shell:

```sh
cargo install --path crates/edx-cli
edx check board.edx.json                 # exit status 1 when the document has errors
edx check --format json board.edx.json   # diagnostics as JSON, for other tools
edx merge design.edx.json rules.edx.json -o merged.edx.json
edx fmt board.edx.json                   # canonical form (section 13 of the spec)
edx hash board.edx.json                  # sha256 of the design, rules and constraints
edx explain EDX-E3005
```

To depend on it from another Rust project before it is on crates.io:

```toml
edx-format = { git = "https://github.com/digimap2000/edx-format" }
```

## Python and JavaScript

Both packages wrap the same Rust core, so their results match it exactly, and both install the same `edx` command.
Documents are plain dictionaries or objects. See [`bindings/python`](bindings/python/README.md) and
[`bindings/js`](bindings/js/README.md).

```sh
pip install ./bindings/python                      # needs a Rust toolchain; builds with maturin
cd bindings/js && npm run build && npm test        # needs the wasm32 target and wasm-bindgen-cli
```

## Running the conformance suite

```sh
cargo test                                         # the reference library, including the whole suite
python tools/conformance.py target/debug/edx       # any implementation with the same command-line interface
python tools/conformance.py node bindings/js/bin/edx.cjs
```

## Licences

- Code: MIT OR Apache-2.0, at your option (`LICENSE-MIT`, `LICENSE-APACHE`).
- Specification text and schemas: Creative Commons Attribution 4.0 (`LICENSE-CC-BY-4.0`).
- Conformance fixtures: CC0 1.0 (`conformance/LICENSE-CC0`), so any project can copy them into its own tests.

## Contributing

Changes to the specification are proposed by pull request. Each change states what it adds, whether it is backwards
compatible within the current major version, and comes with conformance cases.
