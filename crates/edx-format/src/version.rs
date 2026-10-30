//! Layer 2: the `edx` version and required extensions (specification, section 12).

use serde_json::Value;

use crate::diagnostic::{ptr, Diagnostic};

/// The EDX major version this library reads.
pub const MAJOR: u64 = 1;
/// The newest EDX minor version this library knows.
pub const MINOR: u64 = 0;
/// Extensions this library supports. None yet.
pub const SUPPORTED_EXTENSIONS: &[&str] = &[];

pub(crate) struct VersionCheck {
    pub diagnostics: Vec<Diagnostic>,
    /// A newer minor version: check against the schema with unknown members allowed.
    pub newer_minor: bool,
}

pub(crate) fn parse_version(s: &str) -> Option<(u64, u64)> {
    let (major, minor) = s.split_once('.')?;
    let digits = |p: &str| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) && (p == "0" || !p.starts_with('0'));
    if !digits(major) || !digits(minor) {
        return None;
    }
    Some((major.parse().ok()?, minor.parse().ok()?))
}

pub(crate) fn check(doc: &Value) -> VersionCheck {
    let mut out = VersionCheck { diagnostics: Vec::new(), newer_minor: false };
    let Some(object) = doc.as_object() else {
        out.diagnostics.push(Diagnostic::new(
            "EDX-E1001",
            "",
            "the document is not a JSON object, so it has no `edx` version",
        ));
        return out;
    };
    match object.get("edx") {
        None => out.diagnostics.push(Diagnostic::new("EDX-E1001", "", "the document has no `edx` version member")),
        Some(v) => {
            match v.as_str().and_then(parse_version) {
                None => out.diagnostics.push(Diagnostic::new(
                    "EDX-E1001",
                    "/edx",
                    format!("`edx` must be a string \"MAJOR.MINOR\", found {v}"),
                )),
                Some((major, _)) if major != MAJOR => out.diagnostics.push(Diagnostic::new(
                    "EDX-E1002",
                    "/edx",
                    format!("EDX {major}.x is not supported; this reader reads {MAJOR}.x"),
                )),
                Some((_, minor)) if minor > MINOR => {
                    out.newer_minor = true;
                    out.diagnostics.push(Diagnostic::new(
                    "EDX-W1006",
                    "/edx",
                    format!("EDX {MAJOR}.{minor} is newer than this reader ({MAJOR}.{MINOR}); unknown members are ignored"),
                ));
                }
                Some(_) => {}
            }
        }
    }

    let names = |key: &str| -> Vec<String> {
        object
            .get(key)
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect())
            .unwrap_or_default()
    };
    let used = names("extensions_used");
    for (i, name) in names("extensions_required").iter().enumerate() {
        let at = ptr("/extensions_required", i);
        if !SUPPORTED_EXTENSIONS.contains(&name.as_str()) {
            out.diagnostics.push(Diagnostic::new(
                "EDX-E1003",
                at.clone(),
                format!("required extension `{name}` is not supported"),
            ));
        }
        if !used.contains(name) {
            out.diagnostics.push(Diagnostic::new(
                "EDX-E1004",
                at,
                format!("`{name}` is required but not listed in `extensions_used`"),
            ));
        }
    }
    out
}
