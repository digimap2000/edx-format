# edx-format

EDX is a standalone file format and its reference libraries. Keep it usable outside any one tool.

## Changing the format

- The schema exists twice: `schema/1.0/edx.schema.json` and `crates/edx-format/schema/edx.schema.json` (embedded in
  the crate). Keep them byte-identical: edit one, then `cp` it to the other, and check with `cmp`.
- A format change usually touches: `spec/edx.md`, both schema copies, `crates/edx-format/src/model.rs`,
  `semantic.rs`, `canonical.rs`, and conformance cases. Commit aeed5f1 (issue #12) is a good template.
- A model field whose type changes (for example `Vec` to `Option`) also breaks `crates/edx-kicad` (`src/lib.rs`
  and `tests/convert.rs`): update both. The PR says that CopperWorks reads the model (`cw-core/src/edx.rs`) and
  needs updating when it next bumps edx-format in its `Cargo.lock`.
- Before adding a requirement to §4 completeness, check the callers of `incomplete()`: CopperWorks refuses an
  import that fails it.
- A new default value is left out of canonical form (serde `skip_serializing_if`, plus any fix in `canonical.rs`)
  and also needs a line in the "Defaults" list of spec §13.
- A new error code goes in three places: `CODES` in `crates/edx-format/src/diagnostic.rs`, the table in spec §14.3,
  and a conformance case.
- Tagged unions in the model use `#[serde(tag = "kind")]` (see `Placement`); the schema writes them as `oneOf` with
  a `const` kind.

## Conformance cases

- Cases are listed in `conformance/cases.json`, with files at `conformance/{valid,invalid}/<CODE>-<name>.edx.json`.
- A case whose only expected error is EDX-E1000 must fail the schema; every other checked case must pass it.
- The schemas, `cases.json` and the `valid`/`invalid` fixtures round-trip through Python's
  `json.dumps(obj, indent=2, ensure_ascii=False) + "\n"`, so generate and edit them by script, not by hand. The
  exceptions are deliberate: the inputs in `conformance/canonical/` (4-space indent, some out of order on purpose) and the parse-layer
  fixtures (E0001, E0002, E0003).
- A layout or report must carry the design's current hashes in `inputs` (from `edx hash`), or it gets a
  stale-inputs warning. Get them from `target/debug/edx hash` rather than writing them by hand.

## Before pushing

Run what CI runs (`.github/workflows/ci.yml`). The schema and rust jobs, locally:

```sh
cargo fmt --all --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo check --workspace --locked   # plain cargo leaves out the python/js bindings; this checks them too
cargo build --locked -p edx-cli && python3 tools/conformance.py target/debug/edx
uv run --no-project --with jsonschema==4.26.0 python tools/schema_cases.py
uvx check-jsonschema==0.39.0 --check-metaschema schema/1.0/edx.schema.json
```

`jsonschema` isn't installed system-wide; `uv run --with` supplies it. CI also has python, js and msrv jobs, which
take about 2 minutes.

## Merging

The repo merges by squashing: `gh pr merge --squash`. Squash is the only merge method enabled on GitHub.
