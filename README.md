# EDX, the Electronics Design eXchange

EDX is an open, tool-neutral file format for printed circuit board design data. The design side states what the
board is and what it must achieve: outline, stack-up, packages, parts, nets, regions, pours, design-intent
relations, rules and explicit constraints. A layout engine decides how, and returns placement, copper and a report
in the same terms.

An EDX bundle is a directory of readable text files: a `layout.toml` manifest, plus `design.json`, `rules.toml` and
`constraints.json`. JSON is used where files are generated and TOML where they are written by hand.

**Status:** pre-release. The specification, [`spec/edx.md`](spec/edx.md), is a working draft of version 1.0; nothing is stable yet.

## What lives here

| Folder | Contents |
| --- | --- |
| `spec/` | The specification, the normative definition of EDX |
| `schemas/` | JSON Schema (2020-12) for each file kind, normative alongside the spec |
| `conformance/` | Language-neutral test cases: valid, invalid and canonical round-trip bundles |

A reference library (Rust core, with Python and JavaScript packages and a command-line validator) will follow,
published as `edx-format`.

## Licences

- Code: MIT OR Apache-2.0, at your option (`LICENSE-MIT`, `LICENSE-APACHE`).
- Specification text and schemas: Creative Commons Attribution 4.0 (`LICENSE-CC-BY-4.0`).
- Conformance fixtures: CC0 1.0 (`conformance/LICENSE-CC0`), so any project can copy them into its own tests.

## Contributing

Changes to the specification are proposed by pull request. Each change states what it adds, whether it is backwards
compatible within the current major version, and comes with conformance cases.
