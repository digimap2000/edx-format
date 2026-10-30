/** A document: a plain object, as JSON.parse gives it. */
export type Document = Record<string, unknown>;
/** A document as JSON text, UTF-8 bytes, or an object. */
export type Source = string | Uint8Array | ArrayBuffer | Document;

/** One problem found in a document (specification, section 14.2). */
export interface Diagnostic {
  /** A registered code, such as `EDX-E2001`. */
  code: string;
  severity: "error" | "warning";
  /** JSON Pointer (RFC 6901) to the value at fault. */
  pointer: string;
  message: string;
  line?: number;
  column?: number;
}

export interface CheckResult {
  /** True when there are no errors; there may be warnings. */
  ok: boolean;
  diagnostics: Diagnostic[];
  /** The document, once it has passed the schema; null before that. */
  document: Document | null;
  readonly errors: Diagnostic[];
  readonly warnings: Diagnostic[];
}

/** Section hashes, `sha256:<hex>`, for the sections the document has (section 13.2). */
export interface Hashes {
  design?: string;
  rules?: string;
  constraints?: string;
}

/** Thrown when a document has errors; `diagnostics` holds them all. */
export class EdxError extends Error {
  diagnostics: Diagnostic[];
}

/** The JSON Schema documents are checked against, as text. */
export const SCHEMA: string;
/** Every diagnostic code with a one-line summary. */
export const CODES: Record<string, string>;
/** The EDX version this library reads and writes, `MAJOR.MINOR`. */
export const EDX_VERSION: string;

/** Checks a document through all four layers: parse, version and extensions, schema, semantics. */
export function check(source: Source): CheckResult;
/** Reads a document, throwing EdxError if it has errors. */
export function loads(source: Source): Document;
/** Writes a document: two-space indent, plain decimal numbers, final newline; or in canonical form. */
export function dumps(document: Source, options?: { canonical?: boolean }): string;
/** The hashes of a document's design, rules and constraints. Throws EdxError if it has errors. */
export function hashes(source: Source): Hashes;
/** Merges two documents by record identity and checks the result. Conflicts are EDX-E2007. */
export function merge(first: Source, second: Source): CheckResult;
/** One diagnostic as a line of text. */
export function formatDiagnostic(d: Diagnostic): string;
