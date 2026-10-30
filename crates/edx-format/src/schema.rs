//! Layer 3: the published JSON Schema.

use std::sync::OnceLock;

use jsonschema::Validator;
use serde_json::Value;

use crate::diagnostic::Diagnostic;

/// The EDX 1.0 JSON Schema, as published.
pub const SCHEMA: &str = include_str!("../../../schema/1.0/edx.schema.json");

fn schema_value() -> Value {
    value().clone()
}

/// The embedded schema, parsed once.
pub(crate) fn value() -> &'static Value {
    static V: OnceLock<Value> = OnceLock::new();
    V.get_or_init(|| serde_json::from_str(SCHEMA).expect("the embedded schema is valid JSON"))
}

fn strict() -> &'static Validator {
    static V: OnceLock<Validator> = OnceLock::new();
    V.get_or_init(|| jsonschema::validator_for(&schema_value()).expect("the embedded schema compiles"))
}

/// The schema with unknown members allowed, for documents of a newer minor version (section 12.1).
fn lax() -> &'static Validator {
    static V: OnceLock<Validator> = OnceLock::new();
    V.get_or_init(|| {
        let mut schema = schema_value();
        allow_unknown_members(&mut schema);
        jsonschema::validator_for(&schema).expect("the relaxed schema compiles")
    })
}

fn allow_unknown_members(schema: &mut Value) {
    match schema {
        Value::Object(map) => {
            if map.get("additionalProperties") == Some(&Value::Bool(false)) {
                map.remove("additionalProperties");
            }
            map.values_mut().for_each(allow_unknown_members);
        }
        Value::Array(items) => items.iter_mut().for_each(allow_unknown_members),
        _ => {}
    }
}

pub(crate) fn check(doc: &Value, newer_minor: bool) -> Vec<Diagnostic> {
    let validator = if newer_minor { lax() } else { strict() };
    validator
        .iter_errors(doc)
        .map(|e| Diagnostic::new("EDX-E1000", e.instance_path().as_str().to_owned(), shorten(e.to_string())))
        .collect()
}

/// Validator messages quote the offending value and pattern in full; keep them readable.
fn shorten(message: String) -> String {
    const LIMIT: usize = 200;
    match message.char_indices().nth(LIMIT) {
        Some((cut, _)) => format!("{}…", &message[..cut]),
        None => message,
    }
}
