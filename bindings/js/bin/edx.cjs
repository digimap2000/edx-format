#!/usr/bin/env node
"use strict";
// `edx`: check, merge and format EDX documents, as the reference command-line tool does.
const fs = require("node:fs");
const edx = require("../js/index.cjs");

const usage = `edx: check and merge EDX documents (Electronics Design eXchange)

Usage:
  edx check [--format text|json] FILE...
  edx merge [--format text|json] [-o OUT] FIRST SECOND
  edx fmt [-o OUT] FILE
  edx hash [--format text|json] FILE
  edx schema`;

function main(argv) {
  const [command, ...rest] = argv;
  let format = "text";
  let output = null;
  const files = [];
  for (let i = 0; i < rest.length; i++) {
    if (rest[i] === "--format") format = rest[++i];
    else if (rest[i] === "-o" || rest[i] === "--output") output = rest[++i];
    else files.push(rest[i]);
  }
  const read = (f) => (f === "-" ? fs.readFileSync(0) : fs.readFileSync(f));
  const report = (label, r) => {
    for (const d of r.diagnostics) console.log(`${label}: ${edx.formatDiagnostic(d)}`);
    const e = r.errors.length, w = r.warnings.length;
    console.error(`${label}: ${e ? `${e} errors, ${w} warnings` : `ok (${w} warnings)`}`);
  };
  const json = (label, r, merged) => {
    const out = { documents: [{ file: label, ok: r.ok, diagnostics: r.diagnostics }] };
    if (merged) out.result = merged;
    console.log(JSON.stringify(out));
  };
  switch (command) {
    case "check": {
      if (!files.length) break;
      let ok = true;
      for (const f of files) {
        const r = edx.check(read(f));
        ok &&= r.ok;
        (format === "json" ? json : report)(f, r);
      }
      return ok ? 0 : 1;
    }
    case "merge": {
      if (files.length !== 2) break;
      const [a, b] = files.map((f) => edx.check(read(f)));
      let label = `${files[0]} + ${files[1]}`, r;
      if (!a.ok) [label, r] = [files[0], a];
      else if (!b.ok) [label, r] = [files[1], b];
      else r = edx.merge(a.document, b.document);
      const merged = r.ok ? r.document : null;
      if (merged && output) fs.writeFileSync(output, edx.dumps(merged));
      if (format === "json") json(label, r, merged);
      else {
        report(label, r);
        if (merged && !output) process.stdout.write(edx.dumps(merged));
      }
      return r.ok ? 0 : 1;
    }
    case "fmt":
    case "hash": {
      if (files.length !== 1) break;
      try {
        const data = read(files[0]);
        if (command === "fmt") {
          const t = edx.dumps(data, { canonical: true });
          if (output) fs.writeFileSync(output, t);
          else process.stdout.write(t);
        } else {
          const h = edx.hashes(data);
          if (format === "json") console.log(JSON.stringify(h));
          else for (const [k, v] of Object.entries(h)) console.log(`${k.padEnd(12)} ${v}`);
        }
        return 0;
      } catch (e) {
        if (!(e instanceof edx.EdxError)) throw e;
        console.error(`${files[0]}: ${e.message}`);
        return 1;
      }
    }
    case "schema":
      process.stdout.write(edx.SCHEMA);
      return 0;
  }
  console.error(usage);
  return 2;
}

process.exitCode = main(process.argv.slice(2));
