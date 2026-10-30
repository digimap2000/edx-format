//! Layer 1: bytes to JSON, refusing a byte-order mark and duplicate member names.

use std::fmt;

use serde::de::{self, DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};

use crate::diagnostic::Diagnostic;

const DUPLICATE: &str = "duplicate member ";

pub(crate) fn parse(bytes: &[u8]) -> Result<Value, Diagnostic> {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return Err(Diagnostic::new("EDX-E0002", "", "the document begins with a byte-order mark"));
    }
    let text = std::str::from_utf8(bytes).map_err(|e| {
        Diagnostic::new("EDX-E0002", "", format!("not UTF-8: invalid byte at offset {}", e.valid_up_to()))
    })?;
    let mut de = serde_json::Deserializer::from_str(text);
    let value = StrictValue.deserialize(&mut de).and_then(|v| de.end().map(|()| v));
    value.map_err(|e| {
        let code = if e.to_string().starts_with(DUPLICATE) { "EDX-E0003" } else { "EDX-E0001" };
        let text = e.to_string();
        let message = text.split(" at line ").next().unwrap_or(&text);
        let mut d = Diagnostic::new(code, "", message);
        if e.line() > 0 {
            d.line = Some(e.line());
            d.column = Some(e.column());
        }
        d
    })
}

/// Deserializes a `serde_json::Value`, failing on an object with two members of the same name.
struct StrictValue;

impl<'de> DeserializeSeed<'de> for StrictValue {
    type Value = Value;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        deserializer.deserialize_any(StrictVisitor)
    }
}

struct StrictVisitor;

impl<'de> Visitor<'de> for StrictVisitor {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a JSON value")
    }

    fn visit_bool<E>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }

    fn visit_i64<E>(self, v: i64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }

    fn visit_u64<E>(self, v: u64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Value, E> {
        Number::from_f64(v).map(Value::Number).ok_or_else(|| E::custom("number out of range"))
    }

    fn visit_str<E>(self, v: &str) -> Result<Value, E> {
        Ok(Value::String(v.to_owned()))
    }

    fn visit_string<E>(self, v: String) -> Result<Value, E> {
        Ok(Value::String(v))
    }

    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = seq.next_element_seed(StrictValue)? {
            items.push(item);
        }
        Ok(Value::Array(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut object = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if object.contains_key(&key) {
                return Err(de::Error::custom(format!("{DUPLICATE}`{key}`")));
            }
            let value = map.next_value_seed(StrictValue)?;
            object.insert(key, value);
        }
        Ok(Value::Object(object))
    }
}
