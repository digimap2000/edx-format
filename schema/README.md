# Schema

`1.0/edx.schema.json` is the JSON Schema (draft 2020-12) for an EDX 1.x document. It is normative for structure,
alongside the specification; where the two disagree, the specification governs.

It will be published at `https://digimap2000.github.io/edx-format/schema/1.0/edx.schema.json`, its `$id`. Each
minor version of EDX gets its own folder; a published schema never changes.

The schema covers structure only: types, required members, allowed values, the EDX subset of SVG path data, and
namespaced extensions. Rules it cannot express, such as references between records, unique identities, arc radii
and geometry, are the semantic checks of section 14 of the specification.

Objects are strict: an unknown member is an error, which catches misspelt keys. A reader given a document of a newer
minor version than its schema follows section 12.1 of the specification: it ignores members it does not know, with a
warning, rather than refusing the document.

Licence: Creative Commons Attribution 4.0 (see `LICENSE-CC-BY-4.0` at the repository root).
