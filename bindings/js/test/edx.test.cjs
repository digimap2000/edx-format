"use strict";
const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const edx = require("../js/index.cjs");

const suite = path.resolve(__dirname, "../../../conformance");
const file = (p) => fs.readFileSync(path.join(suite, p));

test("a minimal document", () => {
  const r = edx.check('{"edx": "1.0"}');
  assert.equal(r.ok, true);
  assert.deepEqual(r.document, { edx: "1.0" });
});

test("errors carry code and pointer", () => {
  const r = edx.check(file("invalid/E2001-unknown-package.edx.json"));
  assert.deepEqual(r.errors.map((d) => [d.code, d.pointer]), [["EDX-E2001", "/design/parts/1/package"]]);
  assert.throws(() => edx.loads(file("invalid/E2001-unknown-package.edx.json")), edx.EdxError);
});

test("objects, text and bytes are all accepted", () => {
  const doc = edx.loads(file("valid/base.edx.json"));
  assert.equal(edx.check(doc).ok, true);
  assert.equal(edx.check(JSON.stringify(doc)).ok, true);
});

test("written numbers are plain decimals", () => {
  const doc = edx.loads(file("valid/base.edx.json"));
  doc.design.packages[0].pads[0].position = [-0.00005, 0];
  const t = edx.dumps(doc);
  assert.match(t, /-0\.00005/);
  assert.doesNotMatch(t, /e-/);
});

test("canonical form and hashes agree with the suite", () => {
  const cases = Object.fromEntries(JSON.parse(file("cases.json")).cases.map((c) => [c.name, c]));
  const c = cases["canonical-same-meaning"];
  assert.equal(edx.dumps(file(c.input), { canonical: true }), file(c.expect.canonical).toString());
  assert.deepEqual(edx.hashes(file(c.input)), c.expect.hashes);
});

test("merging in either order", () => {
  const a = edx.loads(file("valid/design-only.edx.json"));
  const b = edx.loads(file("examples/rules-only.edx.json"));
  const ab = edx.merge(a, b), ba = edx.merge(b, a);
  assert.equal(ab.ok, true);
  assert.deepEqual(ab.document, ba.document);
});

test("tables", () => {
  assert.match(edx.CODES["EDX-E3005"], /^two pours/);
  assert.equal(edx.EDX_VERSION, "1.0");
  assert.match(JSON.parse(edx.SCHEMA).$id, /schema\/1\.0\/edx\.schema\.json$/);
});
