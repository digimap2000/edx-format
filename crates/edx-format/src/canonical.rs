//! Canonical form and section hashes (specification, section 13).

use std::fmt::Write as _;
use std::sync::OnceLock;

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

/// Writes a document as it stands, its members in the order the schema lists them and numbers in
/// plain decimal: what `to_string` writes, the same whatever order serde_json keeps maps in.
pub(crate) fn write_document(doc: &Document) -> String {
    let value = serde_json::to_value(doc).expect("a document always serialises");
    let mut out = String::new();
    write_value(&value, Some(Schema::root()), 0, Mode::Plain, &mut out);
    out.push('\n');
    out
}

/// The hashes of a document's `design`, `rules` and `constraints`, for those it has.
pub fn hashes(doc: &Document) -> Hashes {
    let doc = canonicalize(doc);
    Hashes {
        design: doc.design.as_ref().map(|s| hash("design", s)),
        rules: doc.rules.as_ref().map(|s| hash("rules", s)),
        constraints: doc.constraints.as_ref().map(|s| hash("constraints", s)),
    }
}

/// `sha256:` and the digest of a section written in canonical form on its own.
fn hash<T: serde::Serialize>(name: &str, section: &T) -> String {
    let value = serde_json::to_value(section).expect("a section always serialises");
    let mut text = String::new();
    write_value(&value, Schema::root().get("properties").and_then(|p| p.get(name)), 0, Mode::Canonical, &mut text);
    text.push('\n');
    let mut s = String::from("sha256:");
    for byte in Sha256::digest(text.as_bytes()) {
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
    match &mut p.fill {
        // Solid is the default, so it is left out unless it carries extensions or extras.
        Some(Fill::Solid { ext }) if ext.extensions.is_empty() && ext.extras.is_empty() => p.fill = None,
        // A hatch's grid is the same turned by a right angle: its angle is normalised to [0, 90).
        Some(Fill::Hatch { angle: a, .. }) => {
            let r = round6(a.rem_euclid(90.0));
            *a = if r >= 90.0 { 0.0 } else { r };
        }
        _ => {}
    }
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

fn rules(_: &mut Rules) {
    // Classes and relation kinds are keyed by name, and every member takes its place as it is
    // written (see `write`): nothing to sort.
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

// ---------------------------------------------------------------- writing

/// Writes an EDX document's JSON in canonical form: two-space indent, LF, a final newline,
/// canonical numbers, members in the order the schema lists them (`extensions`, then `extras`,
/// last; members of objects keyed by name, and members the schema does not know, by name), and the
/// members of `extensions` and `extras` values sorted by name. Numbers inside those are not rounded.
///
/// The order comes from the schema as published, read with its members in order, so it does not
/// depend on whether serde_json keeps maps in insertion order (its `preserve_order` feature).
pub fn write(value: &Value) -> String {
    let mut out = String::new();
    write_value(value, Some(Schema::root()), 0, Mode::Canonical, &mut out);
    out.push('\n');
    out
}

/// Writes JSON as given, in the same layout: two-space indent, LF, a final newline, numbers in
/// plain decimal (never exponent notation, section 2.1), members in the order the map holds them
/// (as they were read only with serde_json's `preserve_order`; see this crate's feature of that name).
pub fn write_plain(value: &Value) -> String {
    let mut out = String::new();
    write_value(value, None, 0, Mode::Plain, &mut out);
    out.push('\n');
    out
}

// ---------------------------------------------------------------- the schema, in order

/// A JSON value with its objects' members in the order written: the schema as published, read
/// without depending on how serde_json orders a map.
enum Ordered {
    Leaf(Value),
    Array(Vec<Ordered>),
    Object(Vec<(String, Ordered)>),
}

impl<'de> serde::Deserialize<'de> for Ordered {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = Ordered;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("JSON")
            }
            fn visit_bool<E>(self, v: bool) -> Result<Ordered, E> {
                Ok(Ordered::Leaf(v.into()))
            }
            fn visit_i64<E>(self, v: i64) -> Result<Ordered, E> {
                Ok(Ordered::Leaf(v.into()))
            }
            fn visit_u64<E>(self, v: u64) -> Result<Ordered, E> {
                Ok(Ordered::Leaf(v.into()))
            }
            fn visit_f64<E>(self, v: f64) -> Result<Ordered, E> {
                Ok(Ordered::Leaf(v.into()))
            }
            fn visit_str<E>(self, v: &str) -> Result<Ordered, E> {
                Ok(Ordered::Leaf(v.into()))
            }
            fn visit_unit<E>(self) -> Result<Ordered, E> {
                Ok(Ordered::Leaf(Value::Null))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut a: A) -> Result<Ordered, A::Error> {
                let mut out = Vec::new();
                while let Some(v) = a.next_element()? {
                    out.push(v);
                }
                Ok(Ordered::Array(out))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, mut a: A) -> Result<Ordered, A::Error> {
                let mut out = Vec::new();
                while let Some((k, v)) = a.next_entry()? {
                    out.push((k, v));
                }
                Ok(Ordered::Object(out))
            }
        }
        d.deserialize_any(V)
    }
}

impl Ordered {
    fn get(&self, key: &str) -> Option<&Ordered> {
        match self {
            Ordered::Object(m) => m.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    fn members(&self) -> &[(String, Ordered)] {
        match self {
            Ordered::Object(m) => m,
            _ => &[],
        }
    }

    fn items(&self) -> &[Ordered] {
        match self {
            Ordered::Array(a) => a,
            _ => &[],
        }
    }

    fn leaf(&self) -> Option<&Value> {
        match self {
            Ordered::Leaf(v) => Some(v),
            _ => None,
        }
    }
}

/// A value the schema is matched against: an object's members are looked at without copying them.
#[derive(Clone, Copy)]
enum Probe<'v> {
    Object(&'v Map<String, Value>),
    Value(&'v Value),
}

impl<'v> Probe<'v> {
    fn get(self, k: &str) -> Option<&'v Value> {
        match self {
            Probe::Object(m) => m.get(k),
            Probe::Value(v) => v.get(k),
        }
    }

    fn value(self) -> Option<&'v Value> {
        match self {
            Probe::Object(_) => None,
            Probe::Value(v) => Some(v),
        }
    }
}

/// Walks the schema beside a value, to find each object's member order and each member's schema.
struct Schema;

impl Schema {
    fn root() -> &'static Ordered {
        static S: OnceLock<Ordered> = OnceLock::new();
        S.get_or_init(|| serde_json::from_str(crate::schema::SCHEMA).expect("the embedded schema is valid JSON"))
    }

    /// A node with its `$ref`s followed.
    fn resolve(mut n: &Ordered) -> &Ordered {
        while let Some(r) = n.get("$ref").and_then(Ordered::leaf).and_then(Value::as_str) {
            match r.strip_prefix("#/$defs/").and_then(|name| Self::root().get("$defs")?.get(name)) {
                Some(next) => n = next,
                None => break,
            }
        }
        n
    }

    /// The nodes that describe `v` under `n`: `n` itself, every `allOf`, `then` and `else`, and of
    /// a `oneOf` or `anyOf` the branch `v` takes (picked by the members whose value the branch
    /// fixes with `const`, as a constraint's `type` or a placement's `kind`).
    fn nodes<'a>(n: &'a Ordered, v: Probe, out: &mut Vec<&'a Ordered>) {
        let n = Self::resolve(n);
        out.push(n);
        for b in n.get("allOf").map(Ordered::items).unwrap_or_default() {
            Self::nodes(b, v, out);
        }
        for k in ["then", "else"] {
            if let Some(b) = n.get(k) {
                Self::nodes(b, v, out);
            }
        }
        for k in ["oneOf", "anyOf"] {
            let branches = n.get(k).map(Ordered::items).unwrap_or_default();
            let fits: Vec<&Ordered> = branches.iter().filter(|b| Self::fits(b, v)).collect();
            let pick = fits.iter().find(|b| Self::fixes(b)).or(fits.first());
            if let Some(b) = pick {
                Self::nodes(b, v, out);
            }
        }
    }

    /// Whether a branch can describe `v`: its type, and every member it fixes with `const`.
    fn fits(b: &Ordered, v: Probe) -> bool {
        let b = Self::resolve(b);
        if let Some(c) = b.get("const").and_then(Ordered::leaf) {
            return v.value() == Some(c);
        }
        let kind = b.get("type").and_then(Ordered::leaf).and_then(Value::as_str);
        let ok = match (kind, v) {
            (Some("object"), Probe::Object(_)) | (None, _) => true,
            (_, Probe::Object(_)) => false,
            (Some("object"), Probe::Value(v)) => v.is_object(),
            (Some("array"), Probe::Value(v)) => v.is_array(),
            (Some("string"), Probe::Value(v)) => v.is_string(),
            (Some("number"), Probe::Value(v)) => v.is_number(),
            (Some("integer"), Probe::Value(v)) => v.is_i64() || v.is_u64(),
            (Some("boolean"), Probe::Value(v)) => v.is_boolean(),
            (Some("null"), Probe::Value(v)) => v.is_null(),
            _ => true,
        };
        ok && b.get("properties").map(Ordered::members).unwrap_or_default().iter().all(|(k, p)| {
            match Self::resolve(p).get("const").and_then(Ordered::leaf) {
                Some(c) => v.get(k) == Some(c),
                None => true,
            }
        })
    }

    /// Whether a branch fixes a member's value: a branch for one kind, not a catch-all.
    fn fixes(b: &Ordered) -> bool {
        let b = Self::resolve(b);
        b.get("properties")
            .map(Ordered::members)
            .unwrap_or_default()
            .iter()
            .any(|(_, p)| Self::resolve(p).get("const").is_some())
    }
}

/// An object's members in the order to write them (see `write`), each with its schema if known.
fn arrange<'a, 'v>(
    map: &'v Map<String, Value>,
    schema: Option<&'a Ordered>,
    mode: Mode,
) -> Vec<(&'v String, &'v Value, Option<&'a Ordered>)> {
    let mut nodes = Vec::new();
    if let Some(s) = schema {
        Schema::nodes(s, Probe::Object(map), &mut nodes);
    }
    let mut known: Vec<&str> = Vec::new();
    for n in &nodes {
        for (k, _) in n.get("properties").map(Ordered::members).unwrap_or_default() {
            if !known.contains(&k.as_str()) {
                known.push(k);
            }
        }
    }
    let child = |k: &str| -> Option<&'a Ordered> {
        nodes
            .iter()
            .find_map(|n| n.get("properties").and_then(|p| p.get(k)))
            .or_else(|| {
                nodes.iter().find_map(|n| n.get("patternProperties").and_then(|p| p.members().first().map(|(_, s)| s)))
            })
            .or_else(|| {
                nodes.iter().find_map(|n| n.get("additionalProperties").filter(|a| matches!(a, Ordered::Object(_))))
            })
    };
    let rank = |k: &str| match k {
        "extensions" => (2, 0),
        "extras" => (3, 0),
        _ => known.iter().position(|n| *n == k).map_or((1, 0), |i| (0, i)),
    };
    let mut entries: Vec<(&String, &Value, Option<&Ordered>)> = map.iter().map(|(k, v)| (k, v, child(k))).collect();
    match mode {
        Mode::Free => entries.sort_by(|a, b| a.0.cmp(b.0)),
        // Unknown members by name in canonical form; as the map holds them otherwise.
        Mode::Canonical => entries.sort_by(|a, b| rank(a.0).cmp(&rank(b.0)).then_with(|| a.0.cmp(b.0))),
        Mode::Plain if schema.is_some() => entries.sort_by_key(|e| rank(e.0)),
        Mode::Plain => {}
    }
    entries
}

/// The schema of an array's `i`th item.
fn item<'a>(schema: Option<&'a Ordered>, array: &Value, i: usize) -> Option<&'a Ordered> {
    let mut nodes = Vec::new();
    Schema::nodes(schema?, Probe::Value(array), &mut nodes);
    nodes
        .iter()
        .find_map(|n| n.get("prefixItems").and_then(|p| p.items().get(i)))
        .or_else(|| nodes.iter().find_map(|n| n.get("items").filter(|s| matches!(s, Ordered::Object(_)))))
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    /// Canonical numbers and order.
    Canonical,
    /// Inside `extensions` or `extras` in canonical form: members sorted, numbers not rounded.
    Free,
    /// As given, numbers not rounded.
    Plain,
}

fn write_value(v: &Value, schema: Option<&Ordered>, depth: usize, mode: Mode, out: &mut String) {
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
            (_, _, Some(f)) if mode != Mode::Canonical => out.push_str(&plain(f)),
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
                write_value(item, self::item(schema, v, i), depth + 1, mode, out);
            }
            indent(out, depth);
            out.push(']');
        }
        Value::Object(map) if map.is_empty() => out.push_str("{}"),
        Value::Object(map) => {
            let entries = arrange(map, if mode == Mode::Free { None } else { schema }, mode);
            out.push('{');
            for (i, (k, item, inner_schema)) in entries.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                indent(out, depth + 1);
                out.push_str(&Value::String(k.clone()).to_string());
                out.push_str(": ");
                let inner = match mode {
                    Mode::Canonical if k == "extensions" || k == "extras" => Mode::Free,
                    m => m,
                };
                write_value(item, if inner == Mode::Free { None } else { inner_schema }, depth + 1, inner, out);
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
    fn pours_of_no_net_and_their_fill() {
        let text = r#"{ "edx": "1.0", "design": { "pours": [
            { "name": "thieving", "layers": ["top"], "outline": ["M 0 0 V 1 H 1 V 0 Z"], "priority": 1,
              "fill": { "kind": "hatch", "width": 0.5, "gap": 0.5, "angle": 135 } },
            { "name": "gnd", "net": "GND", "layers": ["top"], "outline": ["M 2 0 V 1 H 3 V 0 Z"], "priority": 1,
              "fill": { "kind": "solid" } } ] } }"#;
        let (doc, _) = crate::read(text.as_bytes()).expect("reads");
        let pours = &doc.design.as_ref().unwrap().pours;
        assert_eq!(pours[0].net, None);
        assert!(matches!(pours[0].fill, Some(Fill::Hatch { width: 0.5, gap: 0.5, angle: 135.0, .. })));
        // Written plainly, a pour of no net has no `net` member and keeps its fill as given.
        let plain: Value = serde_json::from_str(&crate::to_string(&doc)).unwrap();
        assert!(plain["design"]["pours"][0].get("net").is_none());
        assert_eq!(plain["design"]["pours"][1]["fill"]["kind"], "solid");
        // In canonical form solid fill is left out and the hatch's angle is normalised to [0, 90).
        let canonical = canonicalize(&doc);
        let pours = &canonical.design.as_ref().unwrap().pours;
        let (gnd, thieving) = (&pours[0], &pours[1]);
        assert!(gnd.fill.is_none());
        assert_eq!(thieving.net, None);
        assert!(matches!(thieving.fill, Some(Fill::Hatch { angle: 45.0, .. })));
    }

    #[test]
    fn plain_writing_never_uses_exponents() {
        let v = serde_json::json!({ "a": 0.00000005, "b": 1e20 });
        assert_eq!(write_plain(&v), "{\n  \"a\": 0.00000005,\n  \"b\": 100000000000000000000\n}\n");
    }

    #[test]
    fn extras_keep_their_numbers_and_sort_their_members() {
        let v = serde_json::json!({ "a": 1.5, "extras": { "z": 1.0000000001, "b": [2.0, 1e-9] } });
        let expected = "{\n  \"a\": 1.5,\n  \"extras\": {\n    \"b\": [\n      2,\n      0.000000001\n    ],\n    \"z\": 1.0000000001\n  }\n}\n";
        assert_eq!(write(&v), expected);
    }
}
