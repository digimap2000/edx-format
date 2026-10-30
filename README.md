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

A reference library (Rust core, with Python and JavaScript packages and a command-line validator) will follow,
published as `edx-format`.

## Licences

- Code: MIT OR Apache-2.0, at your option (`LICENSE-MIT`, `LICENSE-APACHE`).
- Specification text and schemas: Creative Commons Attribution 4.0 (`LICENSE-CC-BY-4.0`).
- Conformance fixtures: CC0 1.0 (`conformance/LICENSE-CC0`), so any project can copy them into its own tests.

## Contributing

Changes to the specification are proposed by pull request. Each change states what it adds, whether it is backwards
compatible within the current major version, and comes with conformance cases.
