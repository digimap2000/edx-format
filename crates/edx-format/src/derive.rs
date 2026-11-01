//! Constraints derived from relations and rules, and the constraints in force (section 9.5).
//!
//! Each relation rule key (section 8.3) derives constraints from the relations of its kind:
//!
//! | Rule key | Constraint | Operands |
//! | --- | --- | --- |
//! | `max_track` (`decouples`, `backs`, `filters`, `feeds`, `loads`) | `max_track` | subject, object |
//! | `decouples.square_on_pin` (when true) | `square_on_pin` | subject's part, object |
//! | `decouples.via` | `via_policy` | the relation |
//! | `decouples.adjacent_pins_via` | `via_policy` | the relation, then the object part's other pads on the object's net that no `decouples` relation names |
//! | `clocks.same_layer` (when true) | `same_layer` | path from subject to object |
//! | `clocks.no_vias` (when true) | `no_vias` | path from subject to object |
//! | `clocks.matched_tolerance` | `matched` | the subjects' nets, per crystal and IC (two or more) |
//! | `clocks.quiet_margin` | `quiet`, id `quiet:<crystal>` | the crystal, its load capacitors, the subjects' nets |
//! | `near.max_distance` (with `near.measure`, default `centre`) | `max_distance` | subject, object |
//!
//! A constraint derived from several relations names the first of them, in id order, as its source.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::ids::{constraint_id, escape, relation_id};
use crate::model::*;

/// The constraints derived from the design's relations and the rules' values for their kinds,
/// ordered by id. A document without both derives none.
pub fn derived(doc: &Document) -> Vec<Constraint> {
    let (Some(design), Some(rules)) = (&doc.design, &doc.rules) else { return Vec::new() };
    let mut out: BTreeMap<String, Constraint> = BTreeMap::new();
    let mut add = |c: Constraint| {
        out.entry(c.id.clone()).or_insert(c);
    };
    let pad = |p: &PadRef| Operand::Pad(p.clone());
    let net = |name: &str| Operand::Net { net: name.to_owned(), ext: Ext::default() };
    let part = |reference: &str| Operand::Part { part: reference.to_owned(), ext: Ext::default() };
    // Pads named as the object of a `decouples` relation: each has its own capacitor and via.
    let decoupled: BTreeSet<(&str, &str)> = design
        .relations
        .iter()
        .filter(|r| r.kind == "decouples")
        .map(|r| (r.object.part.as_str(), r.object.pad.as_str()))
        .collect();
    // `clocks` relations by crystal, and by crystal and IC, in id order.
    let mut by_crystal: BTreeMap<&str, Vec<(String, &Relation)>> = BTreeMap::new();
    let mut by_leg_pair: BTreeMap<(&str, &str), Vec<(String, &Relation)>> = BTreeMap::new();

    for relation in &design.relations {
        let Some(values) = rules.relations.get(&relation.kind) else { continue };
        let id = relation_id(relation);
        let rule = |key: &str| format!("{}.{key}", relation.kind);
        let path =
            || Operand::Path { path: vec![relation.subject.clone(), relation.object.clone()], ext: Ext::default() };
        let truthy = |key: &str| values.get(key).and_then(Value::as_bool) == Some(true);
        let pads = || vec![pad(&relation.subject), pad(&relation.object)];
        match relation.kind.as_str() {
            "decouples" | "backs" | "filters" | "feeds" | "loads" => {
                if let Some(v) = values.get("max_track") {
                    add(made("max_track", pads(), Some(v.clone()), &id, rule("max_track")));
                }
            }
            "near" => {
                if let Some(v) = values.get("max_distance") {
                    let measure = values.get("measure").and_then(Value::as_str).unwrap_or("centre");
                    let mut c = made("max_distance", pads(), Some(v.clone()), &id, rule("max_distance"));
                    c.measure = Some(measure.to_owned());
                    add(c);
                }
            }
            _ => {}
        }
        match relation.kind.as_str() {
            "decouples" => {
                if truthy("square_on_pin") {
                    let operands = vec![part(&relation.subject.part), pad(&relation.object)];
                    add(made("square_on_pin", operands, None, &id, rule("square_on_pin")));
                }
                let group = || Operand::Relations { relations: vec![id.clone()], ext: Ext::default() };
                if let Some(v) = values.get("via") {
                    add(made("via_policy", vec![group()], Some(v.clone()), &id, rule("via")));
                }
                if let (Some(v), Some(supply)) = (values.get("adjacent_pins_via"), design.net_of(&relation.object)) {
                    let mut adjacent: Vec<&PadRef> = supply
                        .pads
                        .iter()
                        .filter(|p| p.part == relation.object.part && p.pad != relation.object.pad)
                        .filter(|p| !decoupled.contains(&(p.part.as_str(), p.pad.as_str())))
                        .collect();
                    adjacent.sort_by(|a, b| (&a.part, &a.pad).cmp(&(&b.part, &b.pad)));
                    adjacent.dedup_by(|a, b| a.part == b.part && a.pad == b.pad);
                    if !adjacent.is_empty() {
                        let operands = std::iter::once(group()).chain(adjacent.into_iter().map(pad)).collect();
                        add(made("via_policy", operands, Some(v.clone()), &id, rule("adjacent_pins_via")));
                    }
                }
            }
            "clocks" => {
                if truthy("same_layer") {
                    add(made("same_layer", vec![path()], None, &id, rule("same_layer")));
                }
                if truthy("no_vias") {
                    add(made("no_vias", vec![path()], None, &id, rule("no_vias")));
                }
                by_crystal.entry(relation.subject.part.as_str()).or_default().push((id.clone(), relation));
                by_leg_pair
                    .entry((relation.subject.part.as_str(), relation.object.part.as_str()))
                    .or_default()
                    .push((id.clone(), relation));
            }
            _ => {}
        }
    }

    // Keys whose constraint groups several relations.
    let Some(values) = rules.relations.get("clocks") else { return out.into_values().collect() };
    let first = |legs: &mut Vec<(String, &Relation)>| {
        legs.sort_by(|a, b| a.0.cmp(&b.0));
        legs[0].0.clone()
    };
    let nets_of = |legs: &[(String, &Relation)]| -> Vec<String> {
        let names: BTreeSet<&str> =
            legs.iter().filter_map(|(_, r)| design.net_of(&r.subject)).map(|n| n.name.as_str()).collect();
        names.into_iter().map(str::to_owned).collect()
    };
    if let Some(v) = values.get("matched_tolerance") {
        for legs in by_leg_pair.values_mut() {
            let nets = nets_of(legs);
            if nets.len() >= 2 {
                let source = first(legs);
                let operands = nets.iter().map(|n| net(n)).collect();
                add(made("matched", operands, Some(v.clone()), &source, "clocks.matched_tolerance".into()));
            }
        }
    }
    if let Some(v) = values.get("quiet_margin") {
        for (crystal, legs) in by_crystal.iter_mut() {
            let loads: BTreeSet<&str> = design
                .relations
                .iter()
                .filter(|r| r.kind == "loads" && r.object.part == *crystal)
                .map(|r| r.subject.part.as_str())
                .collect();
            let mut operands = vec![part(crystal)];
            operands.extend(loads.into_iter().map(part));
            operands.extend(nets_of(legs).iter().map(|n| net(n)));
            let source = first(legs);
            let mut c = made("quiet", operands, Some(v.clone()), &source, "clocks.quiet_margin".into());
            c.id = format!("quiet:{}", escape(crystal));
            add(c);
        }
    }
    out.into_values().collect()
}

/// A derived constraint, its id made from its type and operands.
fn made(kind: &str, operands: Vec<Operand>, value: Option<Value>, relation: &str, rule: String) -> Constraint {
    Constraint {
        id: constraint_id(kind, &operands),
        kind: kind.into(),
        operands,
        value,
        measure: None,
        hard: None,
        cost: None,
        origin: Origin::Derived,
        source: Source::Derived { relation: relation.to_owned(), rule, ext: Ext::default() },
        ext: Ext::default(),
    }
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

    const GROUPED: &str = r#"{ "edx": "1.0",
        "design": {
          "nets": [
            { "name": "+3V3", "pads": [ { "part": "U1", "pad": "9" }, { "part": "U1", "pad": "20" }, { "part": "U1", "pad": "11" },
                                        { "part": "U1", "pad": "31" }, { "part": "C5", "pad": "1" }, { "part": "C6", "pad": "1" } ] },
            { "name": "XIN", "pads": [ { "part": "X1", "pad": "1" }, { "part": "U1", "pad": "12" }, { "part": "C1", "pad": "1" } ] },
            { "name": "XOUT", "pads": [ { "part": "X1", "pad": "2" }, { "part": "U1", "pad": "13" }, { "part": "C2", "pad": "1" } ] },
            { "name": "XIN32", "pads": [ { "part": "Y1", "pad": "1" }, { "part": "U1", "pad": "40" } ] } ],
          "relations": [
            { "kind": "decouples", "subject": { "part": "C5", "pad": "1" }, "object": { "part": "U1", "pad": "9" } },
            { "kind": "decouples", "subject": { "part": "C6", "pad": "1" }, "object": { "part": "U1", "pad": "20" } },
            { "kind": "clocks", "subject": { "part": "X1", "pad": "2" }, "object": { "part": "U1", "pad": "13" } },
            { "kind": "clocks", "subject": { "part": "X1", "pad": "1" }, "object": { "part": "U1", "pad": "12" } },
            { "kind": "clocks", "subject": { "part": "Y1", "pad": "1" }, "object": { "part": "U1", "pad": "40" } },
            { "kind": "loads", "subject": { "part": "C1", "pad": "1" }, "object": { "part": "X1", "pad": "1" } },
            { "kind": "loads", "subject": { "part": "C2", "pad": "1" }, "object": { "part": "X1", "pad": "2" } },
            { "kind": "near", "subject": { "part": "R1", "pad": "1" }, "object": { "part": "U1", "pad": "3" } } ] },
        "rules": { "relations": {
            "decouples": { "adjacent_pins_via": "in_link" },
            "clocks": { "matched_tolerance": 0.5, "quiet_margin": 1 },
            "loads": {},
            "near": { "max_distance": 4 } } } }"#;

    #[test]
    fn derives_the_grouped_keys() {
        let doc = crate::check(GROUPED.as_bytes()).document.unwrap();
        let all = derived(&doc);
        let ids: Vec<&str> = all.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "matched:XIN:XOUT",
                "max_distance:R1.1:U1.3",
                "quiet:X1",
                "quiet:Y1",
                "via_policy:decouples%3AC5%2E1%3AU1%2E9:U1.11:U1.31",
                "via_policy:decouples%3AC6%2E1%3AU1%2E20:U1.11:U1.31",
            ]
        );
        let get = |id: &str| all.iter().find(|c| c.id == id).unwrap();
        // Two crystals on one IC are matched apart: Y1 has one leg, so nothing to match.
        let matched = get("matched:XIN:XOUT");
        assert_eq!(matched.value, Some(serde_json::json!(0.5)));
        assert!(matches!(&matched.source, Source::Derived { relation, .. } if relation == "clocks:X1.1:U1.12"));
        assert_eq!(get("max_distance:R1.1:U1.3").measure.as_deref(), Some("centre"));
        let quiet = serde_json::to_value(&get("quiet:X1").operands).unwrap();
        assert_eq!(
            quiet,
            serde_json::json!([{ "part": "X1" }, { "part": "C1" }, { "part": "C2" }, { "net": "XIN" }, { "net": "XOUT" }])
        );
        let quiet32 = serde_json::to_value(&get("quiet:Y1").operands).unwrap();
        assert_eq!(quiet32, serde_json::json!([{ "part": "Y1" }, { "net": "XIN32" }]));
        let via = get("via_policy:decouples%3AC5%2E1%3AU1%2E9:U1.11:U1.31");
        assert!(matches!(&via.source, Source::Derived { rule, .. } if rule == "decouples.adjacent_pins_via"));
    }

    #[test]
    fn derived_constraints_are_valid() {
        for text in [DOC, GROUPED] {
            let mut doc = crate::check(text.as_bytes()).document.unwrap();
            doc.constraints = Some(derived(&doc));
            let text = crate::to_string(&doc);
            let checked = crate::check(text.as_bytes());
            let schema: Vec<_> = checked.diagnostics.iter().filter(|d| d.code == "EDX-E1000").collect();
            assert!(schema.is_empty(), "{schema:?}");
        }
    }
}
