//! Reference reader, checker, merger and writer for EDX, the Electronics Design eXchange format.
//!
//! An EDX document is one JSON file holding any of a board's design, rules, constraints, layout and
//! report. This crate checks a document in the four layers of the specification (section 14):
//! parse, version and extensions, schema, semantics. It reads it into a typed [`Document`], merges
//! documents by record identity, and writes documents back.
//!
//! ```
//! let checked = edx_format::check(br#"{ "edx": "1.0" }"#);
//! assert!(checked.is_ok());
//! let document = checked.document.unwrap();
//! assert_eq!(document.edx, "1.0");
//! ```

pub mod canonical;
mod diagnostic;
pub mod ids;
mod merge;
pub mod model;
mod parse;
mod schema;
mod semantic;
pub mod shape;
mod version;

use serde_json::Value;

pub use canonical::{hashes, to_canonical_string, Hashes};
pub use diagnostic::{Diagnostic, Severity, CODES};
pub use merge::same;
pub use model::Document;
pub use schema::SCHEMA;
pub use version::{MAJOR, MINOR, SUPPORTED_EXTENSIONS};

/// The outcome of checking a document.
#[derive(Clone, Debug, Default)]
pub struct Checked {
    /// The document as JSON, once it parses. Members of a newer minor version are kept here.
    pub value: Option<Value>,
    /// The typed document, once it passes the schema.
    pub document: Option<Document>,
    /// Every error and warning found, in the order found.
    pub diagnostics: Vec<Diagnostic>,
}

impl Checked {
    /// Whether the document has no errors. It may still have warnings.
    pub fn is_ok(&self) -> bool {
        !self.diagnostics.iter().any(Diagnostic::is_error)
    }

    pub fn errors(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics.iter().filter(|d| d.is_error())
    }

    pub fn warnings(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics.iter().filter(|d| !d.is_error())
    }
}

/// Checks a document from its bytes, through all four layers.
pub fn check(bytes: &[u8]) -> Checked {
    match parse::parse(bytes) {
        Ok(value) => check_value(value),
        Err(d) => Checked { value: None, document: None, diagnostics: vec![d] },
    }
}

/// Checks a document that is already JSON: every layer after parsing.
pub fn check_value(value: Value) -> Checked {
    let version = version::check(&value);
    let mut diagnostics = version.diagnostics;
    if diagnostics.iter().any(Diagnostic::is_error) {
        return Checked { value: Some(value), document: None, diagnostics };
    }
    let schema_errors = schema::check(&value, version.newer_minor);
    if !schema_errors.is_empty() {
        diagnostics.extend(schema_errors);
        return Checked { value: Some(value), document: None, diagnostics };
    }
    let document = match serde_json::from_value::<Document>(value.clone()) {
        Ok(d) => d,
        Err(e) => {
            // Only a member of a newer minor version that the model cannot represent reaches here.
            diagnostics.push(Diagnostic::new("EDX-E1000", "", format!("the document cannot be read: {e}")));
            return Checked { value: Some(value), document: None, diagnostics };
        }
    };
    diagnostics.extend(semantic::check(&document, &value, version.newer_minor));
    Checked { value: Some(value), document: Some(document), diagnostics }
}

/// Reads a document, returning it with any warnings, or every error found.
pub fn read(bytes: &[u8]) -> Result<(Document, Vec<Diagnostic>), Vec<Diagnostic>> {
    let checked = check(bytes);
    match checked.document {
        Some(document) if checked.is_ok() => Ok((document, checked.diagnostics)),
        _ => Err(checked.diagnostics.into_iter().filter(Diagnostic::is_error).collect()),
    }
}

/// Merges two documents (section 3.2) and checks the result.
///
/// Each document is checked first; if either has errors the merge does not happen, and their
/// messages say which document they are in. Conflicts are `EDX-E2007`, pointing into the second
/// document. Merging in either order gives the same result.
pub fn merge(first: &Value, second: &Value) -> Checked {
    let mut diagnostics = Vec::new();
    for (which, value) in [("first", first), ("second", second)] {
        for mut d in check_value(value.clone()).diagnostics {
            d.message = format!("{which} document: {}", d.message);
            diagnostics.push(d);
        }
    }
    if diagnostics.iter().any(Diagnostic::is_error) {
        return Checked { value: None, document: None, diagnostics };
    }
    match merge::merge(first, second) {
        Ok(value) => {
            let mut checked = check_value(value);
            checked
                .diagnostics
                .retain(|d| d.is_error() || !diagnostics.iter().any(|w| w.code == d.code && w.pointer == d.pointer));
            diagnostics.extend(checked.diagnostics);
            Checked { diagnostics, ..checked }
        }
        Err(conflicts) => {
            diagnostics.extend(conflicts);
            Checked { value: None, document: None, diagnostics }
        }
    }
}

/// Writes a document as JSON, indented by two spaces, ending with a newline.
pub fn to_string(document: &Document) -> String {
    to_string_value(&serde_json::to_value(document).expect("a document always serialises"))
}

/// Writes JSON as an EDX tool would: indented by two spaces, numbers in plain decimal, ending with a newline.
pub fn to_string_value(value: &Value) -> String {
    canonical::write_plain(value)
}
