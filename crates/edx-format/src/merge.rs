//! Merging two documents by record identity (specification, section 3.2).
//!
//! The merge works on the JSON tree rather than the typed model, so members of a newer minor
//! version, extensions and extras all survive it.

use serde_json::{Map, Value};

use crate::diagnostic::{ptr, Diagnostic};
use crate::ids::relation_id;
use crate::model::Relation;
use crate::shape::round_nm;
use crate::version::parse_version;

/// Whether two values are equal, comparing numbers after rounding to the nanometre (section 2.1).
pub fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => match (x.as_f64(), y.as_f64()) {
            (Some(x), Some(y)) => round_nm(x) == round_nm(y),
            _ => x == y,
        },
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(x, y)| same(x, y)),
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w)))
        }
        _ => a == b,
    }
}

/// How a member's value is merged.
enum Kind {
    /// An array of records, merged by the identity a function gives each one.
    Records(fn(&Value) -> String),
    /// An object of records keyed by identity.
    Keyed,
    /// An object whose members are merged by their own kinds.
    Section(fn(&str) -> Kind),
    /// Any other value: equal in both documents, or a conflict.
    Whole,
}

fn by(member: &'static str) -> fn(&Value) -> String {
    match member {
        "name" => |v| v.get("name").and_then(Value::as_str).unwrap_or_default().to_owned(),
        "id" => |v| v.get("id").and_then(Value::as_str).unwrap_or_default().to_owned(),
        "ref" => |v| v.get("ref").and_then(Value::as_str).unwrap_or_default().to_owned(),
        _ => unreachable!(),
    }
}

fn relation_identity(v: &Value) -> String {
    serde_json::from_value::<Relation>(v.clone()).map(|r| relation_id(&r)).unwrap_or_default()
}

fn top(member: &str) -> Kind {
    match member {
        "design" => Kind::Section(design),
        "rules" => Kind::Section(rules),
        "constraints" => Kind::Records(by("id")),
        "extensions" | "extras" => Kind::Keyed,
        _ => Kind::Whole,
    }
}

fn design(member: &str) -> Kind {
    match member {
        // The stack-up's order matters, so the board is compared whole.
        "packages" => Kind::Records(by("id")),
        "parts" => Kind::Records(by("ref")),
        "nets" | "regions" | "pours" => Kind::Records(by("name")),
        "relations" => Kind::Records(relation_identity),
        "extensions" | "extras" => Kind::Keyed,
        _ => Kind::Whole,
    }
}

fn rules(member: &str) -> Kind {
    match member {
        "classes" | "relations" | "extensions" | "extras" => Kind::Keyed,
        _ => Kind::Whole,
    }
}

/// Merges two documents that have each passed every check. Conflicts are reported as `EDX-E2007`,
/// at the conflicting value in the second document.
pub(crate) fn merge(a: &Value, b: &Value) -> Result<Value, Vec<Diagnostic>> {
    let (Some(a), Some(b)) = (a.as_object(), b.as_object()) else {
        return Err(vec![Diagnostic::new("EDX-E1001", "", "a document must be a JSON object")]);
    };
    let mut conflicts = Vec::new();
    let mut out = Map::new();

    let version = |m: &Map<String, Value>| m.get("edx").and_then(Value::as_str).and_then(parse_version);
    match (version(a), version(b)) {
        (Some(va), Some(vb)) if va.0 == vb.0 => {
            let (major, minor) = va.max(vb);
            out.insert("edx".into(), Value::String(format!("{major}.{minor}")));
        }
        _ => conflicts.push(Diagnostic::new("EDX-E2007", "/edx", "the documents have different major versions")),
    }
    for list in ["extensions_used", "extensions_required"] {
        let mut names: Vec<&str> = [a, b]
            .iter()
            .filter_map(|m| m.get(list).and_then(Value::as_array))
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        names.sort_unstable();
        names.dedup();
        if !names.is_empty() {
            out.insert(list.into(), names.into_iter().map(Value::from).collect());
        }
    }
    for results in ["layout", "report"] {
        if a.contains_key(results) && b.contains_key(results) {
            conflicts.push(Diagnostic::new(
                "EDX-E2007",
                ptr("", results),
                format!("both documents have a `{results}`"),
            ));
        }
    }
    let rest = |m: &Map<String, Value>| {
        m.iter()
            .filter(|(k, _)| !matches!(k.as_str(), "edx" | "extensions_used" | "extensions_required"))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<Map<_, _>>()
    };
    let merged = section(&rest(a), &rest(b), top, "", &mut conflicts);
    out.extend(merged);
    if conflicts.is_empty() {
        Ok(Value::Object(out))
    } else {
        Err(conflicts)
    }
}

fn section(
    a: &Map<String, Value>,
    b: &Map<String, Value>,
    kinds: fn(&str) -> Kind,
    at: &str,
    conflicts: &mut Vec<Diagnostic>,
) -> Map<String, Value> {
    let mut out = Map::new();
    let mut keys: Vec<&String> = a.keys().chain(b.keys()).collect();
    keys.sort_unstable();
    keys.dedup();
    for key in keys {
        let here = ptr(at, key);
        let value = match (a.get(key), b.get(key)) {
            (Some(x), None) | (None, Some(x)) => x.clone(),
            (Some(x), Some(y)) => match (kinds(key), x, y) {
                (Kind::Records(identity), Value::Array(xs), Value::Array(ys)) => {
                    records(xs, ys, identity, &here, conflicts)
                }
                (Kind::Keyed, Value::Object(xs), Value::Object(ys)) => keyed(xs, ys, &here, conflicts),
                (Kind::Section(inner), Value::Object(xs), Value::Object(ys)) => {
                    Value::Object(section(xs, ys, inner, &here, conflicts))
                }
                _ => {
                    if !same(x, y) {
                        conflicts.push(Diagnostic::new(
                            "EDX-E2007",
                            here,
                            format!("`{key}` differs between the documents"),
                        ));
                    }
                    x.clone()
                }
            },
            (None, None) => unreachable!(),
        };
        out.insert(key.clone(), value);
    }
    out
}

fn records(
    a: &[Value],
    b: &[Value],
    identity: fn(&Value) -> String,
    at: &str,
    conflicts: &mut Vec<Diagnostic>,
) -> Value {
    let mut all: Vec<(String, &Value)> = a.iter().map(|v| (identity(v), v)).collect();
    for (i, y) in b.iter().enumerate() {
        let id = identity(y);
        match all.iter().find(|(k, _)| *k == id) {
            Some((_, x)) if !same(x, y) => {
                conflicts.push(Diagnostic::new(
                    "EDX-E2007",
                    ptr(at, i),
                    format!("`{id}` differs between the documents"),
                ));
            }
            Some(_) => {}
            None => all.push((id, y)),
        }
    }
    all.sort_by(|(x, _), (y, _)| x.cmp(y));
    Value::Array(all.into_iter().map(|(_, v)| v.clone()).collect())
}

fn keyed(a: &Map<String, Value>, b: &Map<String, Value>, at: &str, conflicts: &mut Vec<Diagnostic>) -> Value {
    let mut out = a.clone();
    for (k, y) in b {
        match a.get(k) {
            Some(x) if !same(x, y) => {
                conflicts.push(Diagnostic::new(
                    "EDX-E2007",
                    ptr(at, k),
                    format!("`{k}` differs between the documents"),
                ));
            }
            Some(_) => {}
            None => {
                out.insert(k.clone(), y.clone());
            }
        }
    }
    out.sort_keys();
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn numbers_compare_after_rounding() {
        assert!(same(&json!(1.0000000001), &json!(1)));
        assert!(!same(&json!(1.000001), &json!(1)));
    }

    #[test]
    fn records_merge_by_identity_in_either_order() {
        let a = json!({"edx": "1.0", "design": {"nets": [{"name": "B", "pads": []}]}});
        let b = json!({"edx": "1.1", "design": {"nets": [{"name": "A", "pads": []}, {"name": "B", "pads": []}]}});
        let ab = merge(&a, &b).unwrap();
        assert_eq!(ab, merge(&b, &a).unwrap());
        assert_eq!(ab["edx"], "1.1");
        assert_eq!(ab["design"]["nets"][0]["name"], "A");
    }

    #[test]
    fn differing_records_conflict() {
        let a = json!({"edx": "1.0", "rules": {"classes": {"default": {"clearance": 0.2}}}});
        let b = json!({"edx": "1.0", "rules": {"classes": {"default": {"clearance": 0.15}}}});
        let errors = merge(&a, &b).unwrap_err();
        assert_eq!(errors[0].pointer, "/rules/classes/default");
    }
}
