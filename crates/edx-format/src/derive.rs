//! Constraints derived from relations and rules, and the constraints in force (section 9.5).
//!
//! Each relation rule key (section 8.3) derives one constraint per relation of its kind:
//!
//! | Rule key | Constraint | Operands |
//! | --- | --- | --- |
//! | `max_track` (`decouples`, `backs`, `filters`, `feeds`, `loads`) | `max_track` | subject, object |
//! | `decouples.square_on_pin` (when true) | `square_on_pin` | subject's part, object |
//! | `decouples.via` | `via_policy` | the relation |
//! | `clocks.same_layer` (when true) | `same_layer` | path from subject to object |
//! | `clocks.no_vias` (when true) | `no_vias` | path from subject to object |
//! | `near.max_distance` (with `near.measure`) | `max_distance` | subject, object |
//!
//! Keys whose constraint groups several relations (`decouples.adjacent_pins_via`,
//! `clocks.matched_tolerance`, `clocks.quiet_margin`) derive nothing yet.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::ids::{constraint_id, relation_id};
use crate::model::*;

/// The constraints derived from the design's relations and the rules' values for their kinds,
/// ordered by id. A document without both derives none.
pub fn derived(doc: &Document) -> Vec<Constraint> {
    let (Some(design), Some(rules)) = (&doc.design, &doc.rules) else { return Vec::new() };
    let mut out: BTreeMap<String, Constraint> = BTreeMap::new();
    for relation in &design.relations {
        let Some(values) = rules.relations.get(&relation.kind) else { continue };
        let id = relation_id(relation);
        let pad = |p: &PadRef| Operand::Pad(p.clone());
        let path =
            || Operand::Path { path: vec![relation.subject.clone(), relation.object.clone()], ext: Ext::default() };
        let truthy = |key: &str| values.get(key).and_then(Value::as_bool) == Some(true);
        let mut add = |key: &str, kind: &str, operands: Vec<Operand>, value: Option<Value>, measure: Option<String>| {
            let constraint = Constraint {
                id: constraint_id(kind, &operands),
                kind: kind.into(),
                operands,
                value,
                measure,
                hard: None,
                cost: None,
                origin: Origin::Derived,
                source: Source::Derived {
                    relation: id.clone(),
                    rule: format!("{}.{key}", relation.kind),
                    ext: Ext::default(),
                },
                ext: Ext::default(),
            };
            out.entry(constraint.id.clone()).or_insert(constraint);
        };
        let pads = || vec![pad(&relation.subject), pad(&relation.object)];
        match relation.kind.as_str() {
            "decouples" | "backs" | "filters" | "feeds" | "loads" => {
                if let Some(v) = values.get("max_track") {
                    add("max_track", "max_track", pads(), Some(v.clone()), None);
                }
            }
            "near" => {
                if let (Some(v), Some(m)) = (values.get("max_distance"), values.get("measure").and_then(Value::as_str))
                {
                    add("max_distance", "max_distance", pads(), Some(v.clone()), Some(m.to_owned()));
                }
            }
            _ => {}
        }
        match relation.kind.as_str() {
            "decouples" => {
                if truthy("square_on_pin") {
                    let part = Operand::Part { part: relation.subject.part.clone(), ext: Ext::default() };
                    add("square_on_pin", "square_on_pin", vec![part, pad(&relation.object)], None, None);
                }
                if let Some(v) = values.get("via") {
                    let group = Operand::Relations { relations: vec![id.clone()], ext: Ext::default() };
                    add("via", "via_policy", vec![group], Some(v.clone()), None);
                }
            }
            "clocks" => {
                if truthy("same_layer") {
                    add("same_layer", "same_layer", vec![path()], None, None);
                }
                if truthy("no_vias") {
                    add("no_vias", "no_vias", vec![path()], None, None);
                }
            }
            _ => {}
        }
    }
    out.into_values().collect()
}

/// The constraints in force: those derived, with the document's own `constraints` taking the place
/// of any with the same id (an authored constraint wins over a derived one), ordered by id.
pub fn constraints(doc: &Document) -> Vec<Constraint> {
    let mut all: BTreeMap<String, Constraint> = derived(doc).into_iter().map(|c| (c.id.clone(), c)).collect();
    for c in doc.constraints.iter().flatten() {
        all.insert(c.id.clone(), c.clone());
    }
    all.into_values().collect()
}

/// A constraint's value as a length, for the types whose value is one.
pub fn length(c: &Constraint) -> Option<f64> {
    c.value.as_ref().and_then(Value::as_f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = r#"{ "edx": "1.0",
        "design": {
          "relations": [
            { "kind": "decouples", "subject": { "part": "C5", "pad": "1" }, "object": { "part": "U1", "pad": "9" } },
            { "kind": "clocks", "subject": { "part": "X1", "pad": "1" }, "object": { "part": "U1", "pad": "12" } },
            { "kind": "near", "subject": { "part": "R1", "pad": "1" }, "object": { "part": "U1", "pad": "3" } } ] },
        "rules": { "relations": {
            "decouples": { "max_track": 2.5, "square_on_pin": true, "via": "beside_pad" },
            "clocks": { "same_layer": true, "no_vias": false },
            "near": { "max_distance": 4, "measure": "centre" } } },
        "constraints": [
          { "id": "max_track:C5.1:U1.9", "type": "max_track", "operands": [ { "part": "C5", "pad": "1" }, { "part": "U1", "pad": "9" } ],
            "value": 1.5, "origin": "authored", "source": { "document": "review" } } ] }"#;

    #[test]
    fn derives_from_relations_and_rules() {
        let doc = crate::check(DOC.as_bytes()).document.unwrap();
        let ids: Vec<String> = derived(&doc).into_iter().map(|c| c.id).collect();
        assert_eq!(
            ids,
            [
                "max_distance:R1.1:U1.3",
                "max_track:C5.1:U1.9",
                "same_layer:X1.1-U1.12",
                "square_on_pin:C5:U1.9",
                "via_policy:decouples%3AC5%2E1%3AU1%2E9",
            ]
        );
        let all = constraints(&doc);
        let max_track = all.iter().find(|c| c.id == "max_track:C5.1:U1.9").unwrap();
        assert_eq!((length(max_track), max_track.origin), (Some(1.5), Origin::Authored));
        let square = all.iter().find(|c| c.kind == "square_on_pin").unwrap();
        assert!(!square.is_hard());
    }

    #[test]
    fn derived_constraints_are_valid() {
        let mut doc = crate::check(DOC.as_bytes()).document.unwrap();
        doc.constraints = Some(derived(&doc));
        let text = crate::to_string(&doc);
        let checked = crate::check(text.as_bytes());
        assert!(checked.is_ok(), "{:?}", checked.diagnostics);
    }
}
