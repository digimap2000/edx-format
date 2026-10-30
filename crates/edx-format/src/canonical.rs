//! Canonical form and section hashes (specification, section 13).

use std::cmp::Ordering;
use std::fmt::Write as _;

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::ids::relation_id;
use crate::model::*;
use crate::shape::{self, Segment, Subpath};

/// The hashes of the sections a layout engine reads (section 13.2), as `sha256:<hex>`.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct Hashes {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub design: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rules: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constraints: Option<String>,
}

/// A document in canonical form: records sorted, defaults left out, angles normalised, shapes rewritten.
pub fn canonicalize(doc: &Document) -> Document {
    let mut doc = doc.clone();
    doc.extensions_used.sort();
    doc.extensions_required.sort();
    if let Some(d) = &mut doc.design {
        design(d);
    }
    if let Some(r) = &mut doc.rules {
        rules(r);
    }
    if let Some(cs) = &mut doc.constraints {
        cs.iter_mut().for_each(constraint);
        cs.sort_by(|a, b| a.id.cmp(&b.id));
    }
    if let Some(l) = &mut doc.layout {
        layout(l);
    }
    if let Some(r) = &mut doc.report {
        report(r);
    }
    doc
}

/// Writes a document in canonical form.
pub fn to_canonical_string(doc: &Document) -> String {
    write(&serde_json::to_value(canonicalize(doc)).expect("a document always serialises"))
}

/// The hashes of a document's `design`, `rules` and `constraints`, for those it has.
pub fn hashes(doc: &Document) -> Hashes {
    let doc = canonicalize(doc);
    Hashes {
        design: doc.design.as_ref().map(hash),
        rules: doc.rules.as_ref().map(hash),
        constraints: doc.constraints.as_ref().map(hash),
    }
}

/// `sha256:` and the digest of a section written in canonical form on its own.
fn hash<T: serde::Serialize>(section: &T) -> String {
    let value = serde_json::to_value(section).expect("a section always serialises");
    let mut s = String::from("sha256:");
    for byte in Sha256::digest(write(&value).as_bytes()) {
        let _ = write!(s, "{byte:02x}");
    }
    s
}

// ---------------------------------------------------------------- numbers and angles

fn round6(v: f64) -> f64 {
    let r = shape::round_nm(v);
    if r == 0.0 {
        0.0
    } else {
        r
    }
}

/// An angle in [0, 360), to a millionth of a degree (section 2.1).
fn angle(a: f64) -> f64 {
    let a = round6(a.rem_euclid(360.0));
    if a >= 360.0 {
        0.0
    } else {
        a
    }
}

/// The shortest plain decimal for a number, after rounding to six places (sections 2.1 and 13.1).
pub fn number(v: f64) -> String {
    plain(round6(v))
}

/// The shortest plain decimal that reads back as the same number, with no exponent and no `.0`.
fn plain(v: f64) -> String {
    // Rust never writes exponents for f64, and writes the shortest form that reads back the same.
    let s = format!("{}", if v == 0.0 { 0.0 } else { v });
    s.strip_suffix(".0").map(str::to_owned).unwrap_or(s)
}

// ---------------------------------------------------------------- shapes

/// Rewrites a shape in canonical form: numbers rounded, single spaces, `H` and `V` for axis-parallel
/// lines and `L` for the rest, each subpath starting at its lowest point (least y, then least x), outer
/// boundary counter-clockwise and holes clockwise as seen from the top. A shape that does not parse
/// is returned unchanged.
pub fn canonical_shape(d: &str) -> String {
    let Ok(subpaths) = shape::parse(d) else { return d.to_owned() };
    let mut out = String::new();
    for (i, sub) in subpaths.iter().enumerate() {
        let mut ring = cycle(sub);
        // Seen from the top with y downwards, counter-clockwise has a negative shoelace sum.
        let ccw = shape::signed_area(&shape::flatten(sub)) < 0.0;
        if ccw != (i == 0) {
            ring = reverse(&ring);
        }
        let start = (0..ring.len())
            .min_by(|&a, &b| {
                let (pa, pb) = (ring[a].0, ring[b].0);
                pa[1].total_cmp(&pb[1]).then(pa[0].total_cmp(&pb[0]))
            })
            .unwrap_or(0);
        ring.rotate_left(start);
        if !out.is_empty() {
            out.push(' ');
        }
        write_ring(&ring, &mut out);
    }
    out
}

/// A closed ring as (start point, segment from it) pairs, with the closing line made explicit and
/// coordinates rounded.
fn cycle(sub: &Subpath) -> Vec<([f64; 2], Segment)> {
    let r = |p: [f64; 2]| [round6(p[0]), round6(p[1])];
    let mut ring = Vec::new();
    let mut current = r(sub.start);
    for seg in &sub.segments {
        let seg = match *seg {
            Segment::Line { to } => Segment::Line { to: r(to) },
            Segment::Arc { rx, large, sweep, to, .. } => {
                Segment::Arc { rx: round6(rx), ry: round6(rx), rotation: 0.0, large, sweep, to: r(to) }
            }
        };
        let to = end(&seg);
        if to != current || matches!(seg, Segment::Arc { .. }) {
            ring.push((current, seg));
        }
        current = to;
    }
    let start = r(sub.start);
    if current != start {
        ring.push((current, Segment::Line { to: start }));
    }
    ring
}

fn end(seg: &Segment) -> [f64; 2] {
    match seg {
        Segment::Line { to } | Segment::Arc { to, .. } => *to,
    }
}

fn reverse(ring: &[([f64; 2], Segment)]) -> Vec<([f64; 2], Segment)> {
    ring.iter()
        .rev()
        .map(|(from, seg)| {
            let (to, end_point) = (*from, end(seg));
            let seg = match *seg {
                Segment::Line { .. } => Segment::Line { to },
                Segment::Arc { rx, ry, rotation, large, sweep, .. } => {
                    Segment::Arc { rx, ry, rotation, large, sweep: !sweep, to }
                }
            };
            (end_point, seg)
        })
        .collect()
}

fn write_ring(ring: &[([f64; 2], Segment)], out: &mut String) {
    let Some((start, _)) = ring.first() else { return };
    let _ = write!(out, "M {} {}", number(start[0]), number(start[1]));
    // The last segment returns to the start, which Z does on its own when it is a straight line.
    let last = ring.len() - 1;
    for (k, (from, seg)) in ring.iter().enumerate() {
        match *seg {
            Segment::Line { .. } if k == last => {}
            Segment::Line { to } if to[1] == from[1] => {
                let _ = write!(out, " H {}", number(to[0]));
            }
            Segment::Line { to } if to[0] == from[0] => {
                let _ = write!(out, " V {}", number(to[1]));
            }
            Segment::Line { to } => {
                let _ = write!(out, " L {} {}", number(to[0]), number(to[1]));
            }
            Segment::Arc { rx, large, sweep, to, .. } => {
                let _ = write!(
                    out,
                    " A {r} {r} 0 {} {} {} {}",
                    u8::from(large),
                    u8::from(sweep),
                    number(to[0]),
                    number(to[1]),
                    r = number(rx)
                );
            }
        }
    }
    out.push_str(" Z");
}

fn shapes(list: &mut [Shape]) {
    for s in list.iter_mut() {
        *s = canonical_shape(s);
    }
    list.sort();
}

// ---------------------------------------------------------------- sections

fn pads(list: &mut [PadRef]) {
    list.sort_by(|a, b| (&a.part, &a.pad).cmp(&(&b.part, &b.pad)));
}

fn region(r: &mut Region) {
    shapes(&mut r.outline);
    if let RegionLayers::List(layers) = &mut r.layers {
        layers.sort();
    }
    r.excludes.sort();
    r.except_nets.sort();
}

fn pour(p: &mut Pour) {
    shapes(&mut p.outline);
    p.layers.sort();
}

fn design(d: &mut Design) {
    if let Some(b) = &mut d.board {
        shapes(&mut b.outline);
    }
    if let Some(ps) = &mut d.packages {
        for p in ps.iter_mut() {
            shapes(&mut p.courtyard.top);
            if let Some(bottom) = &mut p.courtyard.bottom {
                shapes(bottom);
            }
            if let Some(body) = &mut p.body {
                shapes(&mut body.outline);
            }
            for pad in &mut p.pads {
                pad.rotation = angle(pad.rotation);
                if let Some(o) = &mut pad.outline {
                    *o = canonical_shape(o);
                }
            }
            p.pads.sort_by(|a, b| a.number.cmp(&b.number));
        }
        ps.sort_by(|a, b| a.id.cmp(&b.id));
    }
    if let Some(parts) = &mut d.parts {
        for p in parts.iter_mut() {
            if let Placement::Fixed { angle: a, .. } = &mut p.placement {
                *a = angle(*a);
            }
        }
        parts.sort_by(|a, b| a.reference.cmp(&b.reference));
    }
    if let Some(nets) = &mut d.nets {
        nets.iter_mut().for_each(|n| pads(&mut n.pads));
        nets.sort_by(|a, b| a.name.cmp(&b.name));
    }
    d.regions.iter_mut().for_each(region);
    d.regions.sort_by(|a, b| a.name.cmp(&b.name));
    d.pours.iter_mut().for_each(pour);
    d.pours.sort_by(|a, b| a.name.cmp(&b.name));
    d.relations.sort_by_cached_key(relation_id);
}

fn rules(r: &mut Rules) {
    let defs = &crate::schema::value()["$defs"];
    for (kind, values) in r.relations.iter_mut() {
        let order = &defs["relation_rules"]["properties"][kind.as_str()];
        *values = ordered(std::mem::take(values), order, defs);
    }
}

fn constraint(c: &mut Constraint) {
    if c.cost == Some(1.0) {
        c.cost = None;
    }
    if let Some(h) = c.hard {
        c.hard = None;
        if c.is_hard() != h {
            c.hard = Some(h);
        }
    }
    match (c.kind.as_str(), &mut c.value) {
        ("fixed", Some(Value::Object(v))) => {
            if let Some(a) = v.get("angle").and_then(Value::as_f64) {
                v.insert("angle".into(), angle(a).into());
            }
        }
        ("orientation", Some(Value::Array(angles))) => {
            let mut list: Vec<f64> = angles.iter().filter_map(Value::as_f64).map(angle).collect();
            list.sort_by(f64::total_cmp);
            list.dedup();
            *angles = list.into_iter().map(Value::from).collect();
        }
        _ => {}
    }
    if let Some(Value::Object(v)) = &mut c.value {
        let defs = &crate::schema::value()["$defs"];
        let order = &defs[format!("constraint_{}", c.kind).as_str()]["properties"]["value"];
        *v = ordered(std::mem::take(v), order, defs);
    }
}

fn layout(l: &mut Layout) {
    for p in &mut l.placement {
        p.angle = p.angle.map(angle);
    }
    l.placement.sort_by(|a, b| a.reference.cmp(&b.reference));
    l.regions.iter_mut().for_each(region);
    l.regions.sort_by(|a, b| a.name.cmp(&b.name));
    l.pours.iter_mut().for_each(pour);
    l.pours.sort_by(|a, b| a.name.cmp(&b.name));
    for b in &mut l.groups.blocks {
        b.parts.sort();
        b.angles.iter_mut().for_each(|a| *a = angle(*a));
    }
    l.groups.blocks.sort_by(|a, b| a.name.cmp(&b.name));
    for q in &mut l.groups.quiet {
        q.nets.sort();
    }
    l.groups.quiet.sort_by(|a, b| a.name.cmp(&b.name));
}

fn place(w: &mut Where) {
    w.parts.sort();
    w.nets.sort();
    pads(&mut w.pads);
}

fn report(r: &mut Report) {
    for c in &mut r.constraints {
        if c.cost == Some(1.0) {
            c.cost = None;
        }
        if let Some(w) = &mut c.location {
            place(w);
        }
    }
    r.constraints.sort_by(|a, b| a.id.cmp(&b.id));
    r.unplaced.sort();
    for u in &mut r.unrouted {
        pads(&mut u.pads);
    }
    r.unrouted.sort_by(|a, b| a.net.cmp(&b.net));
    for c in &mut r.clearance {
        c.items.iter_mut().for_each(place);
    }
}

/// Orders an object's members as a schema lists its properties, unknown members last by name.
fn ordered(map: Map<String, Value>, schema: &Value, defs: &Value) -> Map<String, Value> {
    let schema = match schema.get("$ref").and_then(Value::as_str).and_then(|r| r.strip_prefix("#/$defs/")) {
        Some(name) => &defs[name],
        None => schema,
    };
    let known: Vec<&String> =
        schema.get("properties").and_then(Value::as_object).map(|p| p.keys().collect()).unwrap_or_default();
    let rank = |k: &str| known.iter().position(|n| *n == k).unwrap_or(usize::MAX);
    let mut entries: Vec<(String, Value)> = map.into_iter().collect();
    entries.sort_by(|(a, _), (b, _)| match rank(a).cmp(&rank(b)) {
        Ordering::Equal => a.cmp(b),
        other => other,
    });
    entries.into_iter().collect()
}

// ---------------------------------------------------------------- writing

/// Writes JSON in canonical form: two-space indent, LF, a final newline, canonical numbers, and the
/// members of `extensions` and `extras` values sorted by name. Numbers inside those are not rounded.
pub fn write(value: &Value) -> String {
    let mut out = String::new();
    write_value(value, 0, false, &mut out);
    out.push('\n');
    out
}

fn write_value(v: &Value, depth: usize, free: bool, out: &mut String) {
    let indent = |out: &mut String, d: usize| {
        out.push('\n');
        out.extend(std::iter::repeat_n("  ", d));
    };
    match v {
        Value::Null | Value::Bool(_) | Value::String(_) => out.push_str(&v.to_string()),
        Value::Number(n) => match (n.as_i64(), n.as_u64(), n.as_f64()) {
            (Some(i), _, _) => out.push_str(&i.to_string()),
            (_, Some(u), _) => out.push_str(&u.to_string()),
            // Inside extensions and extras numbers are someone else's data: not rounded.
            (_, _, Some(f)) if free => out.push_str(&plain(f)),
            (_, _, Some(f)) => out.push_str(&number(f)),
            _ => out.push_str(&n.to_string()),
        },
        Value::Array(items) if items.is_empty() => out.push_str("[]"),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                indent(out, depth + 1);
                write_value(item, depth + 1, free, out);
            }
            indent(out, depth);
            out.push(']');
        }
        Value::Object(map) if map.is_empty() => out.push_str("{}"),
        Value::Object(map) => {
            let mut entries: Vec<(&String, &Value)> = map.iter().collect();
            if free {
                entries.sort_by_key(|(k, _)| *k);
            }
            out.push('{');
            for (i, (k, item)) in entries.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                indent(out, depth + 1);
                out.push_str(&Value::String(k.clone()).to_string());
                out.push_str(": ");
                write_value(item, depth + 1, free || k == "extensions" || k == "extras", out);
            }
            indent(out, depth);
            out.push('}');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers() {
        assert_eq!(number(1.0), "1");
        assert_eq!(number(-0.0000001), "0");
        assert_eq!(number(0.1 + 0.2), "0.3");
        assert_eq!(number(1.0000005), "1.000001");
        assert_eq!(number(-2.5e-7), "0");
        assert_eq!(number(0.0000005), "0.000001");
        assert_eq!(number(123456789.0), "123456789");
        assert_eq!(angle(-90.0), 270.0);
        assert_eq!(angle(359.9999999), 0.0);
    }

    #[test]
    fn shapes_are_rewritten() {
        // Clockwise as seen from the top, starting elsewhere, with an explicit closing line.
        let cw = "M 10,0 L 10 10 L 0 10 L 0 0 L 10 0 Z";
        assert_eq!(canonical_shape(cw), "M 0 0 V 10 H 10 V 0 Z");
        assert_eq!(canonical_shape("M 0 0 V 10 H 10 V 0 Z"), "M 0 0 V 10 H 10 V 0 Z");
        // A hole runs clockwise.
        let holed = canonical_shape("M 0 0 V 10 H 10 V 0 Z M 2 2 V 4 H 4 V 2 Z");
        assert_eq!(holed, "M 0 0 V 10 H 10 V 0 Z M 2 2 H 4 V 4 H 2 Z");
    }

    #[test]
    fn arcs_reverse_with_their_sweep() {
        let ccw = "M 1 0 A 1 1 0 0 0 0 1 V 19 A 1 1 0 0 0 1 20 H 29 A 1 1 0 0 0 30 19 V 1 A 1 1 0 0 0 29 0 Z";
        assert_eq!(canonical_shape(ccw), ccw);
        let cw = "M 1 0 H 29 A 1 1 0 0 1 30 1 V 19 A 1 1 0 0 1 29 20 H 1 A 1 1 0 0 1 0 19 V 1 A 1 1 0 0 1 1 0 Z";
        assert_eq!(canonical_shape(cw), ccw);
    }

    #[test]
    fn extras_keep_their_numbers_and_sort_their_members() {
        let v = serde_json::json!({ "a": 1.5, "extras": { "z": 1.0000000001, "b": [2.0, 1e-9] } });
        let expected = "{\n  \"a\": 1.5,\n  \"extras\": {\n    \"b\": [\n      2,\n      0.000000001\n    ],\n    \"z\": 1.0000000001\n  }\n}\n";
        assert_eq!(write(&v), expected);
    }
}
