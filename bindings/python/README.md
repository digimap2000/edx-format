# edx-format for Python

Reader, checker, merger and writer for [EDX](https://github.com/digimap2000/edx-format), the Electronics Design
eXchange format. The work is done by the Rust reference library, so results match every other `edx-format` package
exactly.

```sh
pip install edx-format          # once published; until then: pip install ./bindings/python
```

```python
import edx_format

result = edx_format.check_file("board.edx.json")
for d in result.diagnostics:
    print(d)                     # error EDX-E2001: /design/parts/3/package: no package `SOT23`

doc = edx_format.load("board.edx.json")              # a dict; raises EdxError if the document has errors
doc["design"]["parts"][0]["value"] = "220nF"
text = edx_format.dumps(doc)                          # two-space indent, plain decimal numbers
text = edx_format.dumps(doc, canonical=True)          # canonical form: one spelling per meaning
edx_format.hashes(doc)                                # {"design": "sha256:…", "rules": …, "constraints": …}

merged = edx_format.merge(design_doc, rules_doc)      # CheckResult; merged.document when merged.ok
```

The package also installs the `edx` command (`edx check`, `edx merge`, `edx fmt`, `edx hash`, `edx schema`), the
same interface as the Rust tool.

Licence: MIT OR Apache-2.0.
