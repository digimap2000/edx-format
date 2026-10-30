//! The native half of the `edx_format` Python package. Documents cross as JSON text; the Python half
//! turns them into dictionaries and dataclasses.

use pyo3::prelude::*;
use serde_json::{json, Value};

fn outcome(checked: &edx_format::Checked) -> String {
    json!({
        "ok": checked.is_ok(),
        "diagnostics": checked.diagnostics,
        "document": checked.value.as_ref().filter(|_| checked.document.is_some()),
    })
    .to_string()
}

/// Checks a document given as bytes. Returns `{"ok", "diagnostics", "document"}` as JSON.
#[pyfunction]
fn check(data: &[u8]) -> String {
    outcome(&edx_format::check(data))
}

/// Merges two documents given as JSON text. Returns the same shape as `check`.
#[pyfunction]
fn merge(first: &str, second: &str) -> PyResult<String> {
    let parse =
        |s: &str| serde_json::from_str::<Value>(s).map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()));
    Ok(outcome(&edx_format::merge(&parse(first)?, &parse(second)?)))
}

/// Checks a document and, when it has no errors and is of a version this library knows, returns
/// `{"ok": true, "canonical": text, "hashes": {...}}`; otherwise the result of `check`.
#[pyfunction]
fn canonical(data: &[u8]) -> String {
    let checked = edx_format::check(data);
    let newer = checked.diagnostics.iter().any(|d| d.code == "EDX-W1006");
    match &checked.document {
        Some(doc) if checked.is_ok() && !newer => json!({
            "ok": true,
            "diagnostics": checked.diagnostics,
            "canonical": edx_format::to_canonical_string(doc),
            "hashes": edx_format::hashes(doc),
        })
        .to_string(),
        _ => {
            let mut v: Value = serde_json::from_str(&outcome(&checked)).expect("valid JSON");
            if checked.is_ok() {
                v["ok"] = false.into();
                v["reason"] = "a document of a newer minor version has no canonical form this library can write".into();
            }
            v.to_string()
        }
    }
}

/// Writes a document given as JSON text: two-space indent, plain decimal numbers, final newline.
#[pyfunction]
fn write(text: &str) -> PyResult<String> {
    let value: Value =
        serde_json::from_str(text).map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    Ok(edx_format::to_string_value(&value))
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(check, m)?)?;
    m.add_function(wrap_pyfunction!(merge, m)?)?;
    m.add_function(wrap_pyfunction!(canonical, m)?)?;
    m.add_function(wrap_pyfunction!(write, m)?)?;
    m.add("SCHEMA", edx_format::SCHEMA)?;
    m.add("CODES", edx_format::CODES.to_vec())?;
    m.add("VERSION", env!("CARGO_PKG_VERSION"))?;
    m.add("EDX_VERSION", format!("{}.{}", edx_format::MAJOR, edx_format::MINOR))?;
    Ok(())
}
