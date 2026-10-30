use std::fmt;

use serde::Serialize;

/// Whether a diagnostic stops a document from being read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

/// One problem found in a document (specification, section 14.2).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Diagnostic {
    /// A registered code, such as `EDX-E2001`.
    pub code: &'static str,
    pub severity: Severity,
    /// JSON Pointer (RFC 6901) to the value at fault.
    pub pointer: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<usize>,
}

impl Diagnostic {
    pub(crate) fn new(code: &'static str, pointer: impl Into<String>, message: impl Into<String>) -> Self {
        let severity = if code.starts_with("EDX-W") { Severity::Warning } else { Severity::Error };
        Diagnostic { code, severity, pointer: pointer.into(), message: message.into(), line: None, column: None }
    }

    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let level = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        let at = if self.pointer.is_empty() { "(document)" } else { &self.pointer };
        write!(f, "{level} {}: {at}: {}", self.code, self.message)?;
        if let (Some(line), Some(column)) = (self.line, self.column) {
            write!(f, " (line {line}, column {column})")?;
        }
        Ok(())
    }
}

/// Builds JSON Pointers one segment at a time.
pub(crate) fn ptr(base: &str, segment: impl fmt::Display) -> String {
    let segment = segment.to_string().replace('~', "~0").replace('/', "~1");
    format!("{base}/{segment}")
}

/// Every registered code, with a one-line summary (specification, section 14.3).
pub const CODES: &[(&str, &str)] = &[
    ("EDX-E0001", "not valid JSON"),
    ("EDX-E0002", "not UTF-8, or begins with a byte-order mark"),
    ("EDX-E0003", "an object has two members with the same name"),
    ("EDX-E1000", "the document does not match the schema"),
    ("EDX-E1001", "`edx` is missing or is not a version"),
    ("EDX-E1002", "unsupported major version"),
    ("EDX-E1003", "a required extension is not supported"),
    ("EDX-E1004", "`extensions_required` names an extension missing from `extensions_used`"),
    ("EDX-E1005", "an extension name is used in the document but missing from `extensions_used`"),
    ("EDX-W1006", "the document is of a newer minor version; unknown members are ignored"),
    ("EDX-W1007", "an extension that is used but not required is not supported; its data is ignored"),
    ("EDX-E2001", "a part names a package that does not exist"),
    ("EDX-E2002", "a reference names a part that does not exist"),
    ("EDX-E2003", "a pad reference names a pad its part's package does not have"),
    ("EDX-E2004", "a pad belongs to more than one net"),
    ("EDX-E2005", "a net names a class that is not defined"),
    ("EDX-E2006", "two records of one kind share an identity"),
    ("EDX-E2007", "merge conflict: one identity, two different records or values"),
    ("EDX-E2008", "the document is not complete (section 4) where it must be"),
    ("EDX-E2009", "a stated relation id differs from the derived id"),
    ("EDX-E2010", "a reference names a net that does not exist"),
    ("EDX-E2011", "a reference names a layer that does not exist"),
    ("EDX-E2012", "a reference names a region, block or relation that does not exist"),
    ("EDX-E3001", "an arc whose radii differ, or are zero"),
    ("EDX-E3002", "a subpath crosses itself or encloses no area"),
    ("EDX-E3003", "a hole is not inside its outer boundary, or holes overlap"),
    ("EDX-E3004", "the stack-up does not start with `top` and end with `bottom`"),
    ("EDX-E3005", "two pours of different nets overlap on a layer at the same priority"),
    ("EDX-E3006", "a circular pad whose width and height differ"),
    ("EDX-E4001", "a relation kind with no rule, in a document that must be complete"),
    ("EDX-E4002", "two authored constraints contradict each other"),
];
