//! Runs the language-neutral conformance suite in `conformance/` (see its README for the rules).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde_json::Value;

fn suite() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance")
}

fn pairs(list: &Value) -> BTreeSet<(String, String)> {
    list.as_array()
        .into_iter()
        .flatten()
        .map(|d| (d["code"].as_str().unwrap().to_owned(), d["pointer"].as_str().unwrap().to_owned()))
        .collect()
}

fn read(path: &str) -> Value {
    let bytes = std::fs::read(suite().join(path)).unwrap();
    edx_format::check(&bytes).value.expect("a merge input parses")
}

/// Checks one case, returning a description of each way it fails.
fn run(case: &Value) -> Vec<String> {
    let expected_errors = pairs(&case["expect"]["errors"]);
    let expected_warnings = pairs(&case["expect"]["warnings"]);
    let mut problems = Vec::new();
    let outcomes = if let Some(inputs) = case["merge"].as_array() {
        let (a, b) = (read(inputs[0].as_str().unwrap()), read(inputs[1].as_str().unwrap()));
        let (ab, ba) = (edx_format::merge(&a, &b), edx_format::merge(&b, &a));
        if ab.value != ba.value {
            problems.push("merging in the two orders gives different results".to_owned());
        }
        vec![ab, ba]
    } else {
        let bytes = std::fs::read(suite().join(case["input"].as_str().unwrap())).unwrap();
        vec![edx_format::check(&bytes)]
    };
    if let Some(expected) = case["expect"]["canonical"].as_str() {
        let expected = std::fs::read_to_string(suite().join(expected)).unwrap();
        match outcomes[0].document.as_ref() {
            Some(document) if edx_format::to_canonical_string(document) != expected => {
                problems.push("the canonical form differs from the expected file".to_owned())
            }
            Some(_) => {}
            None => problems.push("no document to write in canonical form".to_owned()),
        }
    }
    if let Some(expected) = case["expect"].get("hashes") {
        if let Some(document) = outcomes[0].document.as_ref() {
            let hashes = serde_json::to_value(edx_format::hashes(document)).unwrap();
            if &hashes != expected {
                problems.push(format!("expected hashes {expected}, got {hashes}"));
            }
        }
    }
    for checked in outcomes {
        let errors: BTreeSet<(String, String)> =
            checked.errors().map(|d| (d.code.to_owned(), d.pointer.clone())).collect();
        let warnings: BTreeSet<(String, String)> =
            checked.warnings().map(|d| (d.code.to_owned(), d.pointer.clone())).collect();
        let schema_match = |(code, pointer): &(String, String)| {
            code == "EDX-E1000"
                && errors.iter().any(|(c, p)| c == code && (p == pointer || p.starts_with(&format!("{pointer}/"))))
        };
        let ok = if expected_errors.iter().any(|(c, _)| c == "EDX-E1000") {
            expected_errors.iter().all(schema_match) && errors.iter().all(|(c, _)| c == "EDX-E1000")
        } else {
            errors == expected_errors
        };
        if !ok {
            let detail: Vec<String> = checked.errors().map(ToString::to_string).collect();
            problems.push(format!("expected errors {expected_errors:?}, got:\n    {}", detail.join("\n    ")));
        }
        let missing: Vec<_> = expected_warnings.difference(&warnings).collect();
        if !missing.is_empty() {
            problems.push(format!("missing warnings {missing:?}"));
        }
    }
    problems
}

#[test]
fn conformance_suite() {
    let cases: Value = serde_json::from_slice(&std::fs::read(suite().join("cases.json")).unwrap()).unwrap();
    let cases = cases["cases"].as_array().or(cases.as_array()).unwrap();
    let mut failures = Vec::new();
    for case in cases {
        for problem in run(case) {
            failures.push(format!("{}: {problem}", case["name"].as_str().unwrap()));
        }
    }
    assert!(failures.is_empty(), "{} of {} cases failed:\n{}", failures.len(), cases.len(), failures.join("\n"));
}

/// Whether `written` holds everything in `original`: no member, extension or extra lost. The writer
/// leaves out empty lists, which mean the same as absent ones.
fn covers(original: &Value, written: &Value) -> bool {
    let empty = |v: &Value| v.as_array().is_some_and(Vec::is_empty) || v.as_object().is_some_and(|m| m.is_empty());
    match (original, written) {
        (Value::Object(a), Value::Object(b)) => a.iter().all(|(k, v)| match b.get(k) {
            Some(w) => covers(v, w),
            None => empty(v),
        }),
        (Value::Array(a), Value::Array(b)) => a.len() == b.len() && a.iter().zip(b).all(|(v, w)| covers(v, w)),
        _ => edx_format::same(original, written),
    }
}

#[test]
fn valid_documents_round_trip_through_the_model() {
    for folder in ["valid", "examples"] {
        for entry in std::fs::read_dir(suite().join(folder)).unwrap() {
            let path = entry.unwrap().path();
            let name = path.display().to_string();
            let (document, _) =
                edx_format::read(&std::fs::read(&path).unwrap()).unwrap_or_else(|e| panic!("{name}: {e:?}"));
            let written = edx_format::to_string(&document);
            let again = edx_format::check(written.as_bytes());
            assert!(again.is_ok(), "{name}: the written document has errors: {:?}", again.diagnostics);
            let original: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            // A newer minor version's unknown members are dropped by design.
            if original["edx"] == "1.0" {
                assert!(covers(&original, again.value.as_ref().unwrap()), "{name}: writing lost data");
            }
        }
    }
}
