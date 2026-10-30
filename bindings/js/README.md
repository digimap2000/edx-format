# edx-format for JavaScript

Reader, checker, merger and writer for [EDX](https://github.com/digimap2000/edx-format), the Electronics Design
eXchange format. The work is done by the Rust reference library compiled to WebAssembly, so results match every
other `edx-format` package exactly. Works in Node.js 18 and later, and in bundlers that load WebAssembly modules
(webpack 5, or Vite with `vite-plugin-wasm`).

```js
import { check, loads, dumps, hashes, merge } from "edx-format";

const result = check(fs.readFileSync("board.edx.json"));   // text, bytes or an object
for (const d of result.diagnostics) console.log(d.code, d.pointer, d.message);

const doc = loads(text);                     // an object; throws EdxError if the document has errors
dumps(doc);                                  // two-space indent, plain decimal numbers
dumps(doc, { canonical: true });             // canonical form: one spelling per meaning
hashes(doc);                                 // { design: "sha256:…", rules: …, constraints: … }
merge(designDoc, rulesDoc);                  // { ok, diagnostics, document }
```

The package also installs the `edx` command with the same interface as the Rust tool.

## Building from source

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130   # the version in Cargo.lock
npm run build && npm test
```

Licence: MIT OR Apache-2.0.
