# Schemas

JSON Schema (draft 2020-12) for each EDX file kind, normative alongside the specification:

- `layout`: the bundle manifest (`layout.toml`)
- `design`: the design description (`design.json`)
- `rules`: electrical and layout rules (`rules.toml`)
- `constraints`: explicit constraints (`constraints.json`)
- `layout-export` and `report`: what a layout engine returns
- `common`: shared definitions

The TOML files are validated against their schemas through TOML's JSON data model.

Licence: Creative Commons Attribution 4.0 (see `LICENSE-CC-BY-4.0` at the repository root).
