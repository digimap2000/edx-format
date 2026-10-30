"use strict";
// The public API, built on the WebAssembly core. Documents are plain objects, as JSON.parse gives them.

class EdxError extends Error {
  constructor(diagnostics, message) {
    const errors = diagnostics.filter((d) => d.severity === "error");
    super(message || errors.map(format).join("\n") || "the document cannot be used");
    this.name = "EdxError";
    this.diagnostics = diagnostics;
  }
}

function format(d) {
  let text = `${d.severity} ${d.code}: ${d.pointer || "(document)"}: ${d.message}`;
  if (d.line != null) text += ` (line ${d.line}, column ${d.column})`;
  return text;
}

const encoder = new TextEncoder();

function bytes(source) {
  if (typeof source === "string") return encoder.encode(source);
  if (source instanceof Uint8Array) return source;
  if (source instanceof ArrayBuffer) return new Uint8Array(source);
  return encoder.encode(JSON.stringify(source));
}

function text(source) {
  if (typeof source === "string") return source;
  if (source instanceof Uint8Array || source instanceof ArrayBuffer) return new TextDecoder().decode(source);
  return JSON.stringify(source);
}

function result(raw) {
  const r = JSON.parse(raw);
  return {
    ok: r.ok,
    diagnostics: r.diagnostics,
    document: r.document ?? null,
    get errors() { return this.diagnostics.filter((d) => d.severity === "error"); },
    get warnings() { return this.diagnostics.filter((d) => d.severity !== "error"); },
  };
}

module.exports = function api(native) {
  function canonicalOf(source) {
    const r = JSON.parse(native.canonical(bytes(source)));
    if (!r.ok) throw new EdxError(r.diagnostics, r.reason);
    return r;
  }
  return {
    EdxError,
    formatDiagnostic: format,
    SCHEMA: native.schema(),
    CODES: JSON.parse(native.codes()),
    EDX_VERSION: native.edxVersion(),
    check: (source) => result(native.check(bytes(source))),
    loads(source) {
      const r = result(native.check(bytes(source)));
      if (!r.ok || r.document === null) throw new EdxError(r.diagnostics);
      return r.document;
    },
    dumps(document, options = {}) {
      return options.canonical ? canonicalOf(document).canonical : native.write(text(document));
    },
    hashes: (source) => canonicalOf(source).hashes,
    merge: (first, second) => result(native.merge(text(first), text(second))),
  };
};
