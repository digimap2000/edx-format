//! Layer 4: the rules a schema cannot express (specification, section 14.3).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use geo::MultiPolygon;
use serde_json::Value;

use crate::diagnostic::{ptr, Diagnostic};
use crate::ids::{pad_id, relation_id};
use crate::model::*;
use crate::shape;
use crate::version::SUPPORTED_EXTENSIONS;

/// `newer_minor`: the document is of a newer minor version, so its hashes cannot be computed here.
pub(crate) fn check(doc: &Document, raw: &Value, newer_minor: bool) -> Vec<Diagnostic> {
    let mut c = Checker::new(doc);
    c.extensions(raw);
    c.identities();
    c.references();
    c.geometry();
    c.completeness();
    c.contradictions();
    if !newer_minor {
        c.inputs();
    }
    c.out
}

fn is_extension_name(s: &str) -> bool {
    s.contains('.')
}

struct Checker<'a> {
    doc: &'a Document,
    out: Vec<Diagnostic>,
    /// Part reference to the pad numbers of its package (None when the package is unknown).
    parts: Option<HashMap<&'a str, Option<HashSet<&'a str>>>>,
    nets: Option<HashSet<&'a str>>,
    layers: Option<HashSet<&'a str>>,
    /// Region names and relation ids, when the document has a design.
    regions: Option<HashSet<&'a str>>,
    blocks: HashSet<String>,
    relations: Option<HashSet<String>>,
    /// Extensions used but not supported, whose data is ignored.
    ignored: HashSet<&'a str>,
}

impl<'a> Checker<'a> {
    fn new(doc: &'a Document) -> Self {
        let design = doc.design.as_ref();
        let packages: Option<HashMap<&str, HashSet<&str>>> = design.and_then(|d| d.packages.as_ref()).map(|ps| {
            ps.iter().map(|p| (p.id.as_str(), p.pads.iter().map(|pad| pad.number.as_str()).collect())).collect()
        });
        let parts = design.and_then(|d| d.parts.as_ref()).map(|parts| {
            parts
                .iter()
                .map(|p| (p.reference.as_str(), packages.as_ref().and_then(|pk| pk.get(p.package.as_str()).cloned())))
                .collect()
        });
        let nets = design.and_then(|d| d.nets.as_ref()).map(|ns| ns.iter().map(|n| n.name.as_str()).collect());
        let layers = design.and_then(|d| d.board.as_ref()).map(|b| b.stackup.iter().map(|l| l.name.as_str()).collect());
        let regions = design.map(|d| d.regions.iter().map(|r| r.name.as_str()).collect());
        let mut blocks: HashSet<String> = HashSet::new();
        for c in doc.constraints.iter().flatten().filter(|c| c.kind == "block") {
            if let Some(name) = c.value.as_ref().and_then(|v| v.get("name")).and_then(Value::as_str) {
                blocks.insert(name.to_owned());
            }
        }
        if let Some(layout) = &doc.layout {
            blocks.extend(layout.groups.blocks.iter().map(|b| b.name.clone()));
        }
        let relations = design.map(|d| d.relations.iter().map(relation_id).collect());
        Checker { doc, out: Vec::new(), parts, nets, layers, regions, blocks, relations, ignored: HashSet::new() }
    }

    fn push(&mut self, code: &'static str, pointer: impl Into<String>, message: impl Into<String>) {
        self.out.push(Diagnostic::new(code, pointer, message));
    }

    // ------------------------------------------------------------ extensions (section 12.3)

    fn extensions(&mut self, raw: &Value) {
        let used: HashSet<&str> = self.doc.extensions_used.iter().map(String::as_str).collect();
        for (i, name) in self.doc.extensions_used.iter().enumerate() {
            if !SUPPORTED_EXTENSIONS.contains(&name.as_str()) && !self.doc.extensions_required.contains(name) {
                self.ignored.insert(name);
                self.push(
                    "EDX-W1007",
                    ptr("/extensions_used", i),
                    format!("extension `{name}` is not supported; its data is ignored"),
                );
            }
        }
        let mut found: Vec<(String, String)> = Vec::new();
        collect_extension_members(raw, "", &mut found);
        if let Some(design) = &self.doc.design {
            for (i, p) in design.packages.iter().flatten().enumerate() {
                if is_extension_name(&p.class) {
                    found.push((p.class.clone(), format!("/design/packages/{i}/class")));
                }
            }
            for (i, r) in design.relations.iter().enumerate() {
                if is_extension_name(&r.kind) {
                    found.push((r.kind.clone(), format!("/design/relations/{i}/kind")));
                }
            }
        }
        for (i, c) in self.doc.constraints.iter().flatten().enumerate() {
            if is_extension_name(&c.kind) {
                found.push((c.kind.clone(), format!("/constraints/{i}/type")));
            }
        }
        if let Some(rules) = &self.doc.rules {
            for kind in rules.relations.keys().filter(|k| is_extension_name(k)) {
                found.push((kind.clone(), ptr("/rules/relations", kind)));
            }
        }
        for (name, at) in found {
            if !used.contains(name.as_str()) {
                self.push("EDX-E1005", at, format!("extension `{name}` is used but not listed in `extensions_used`"));
            }
        }
    }

    // ------------------------------------------------------------ identities (section 3.1)

    fn identities(&mut self) {
        let doc = self.doc;
        let dup = |out: &mut Vec<Diagnostic>, what: &str, names: Vec<(String, String)>| {
            let mut seen = HashSet::new();
            for (name, at) in names {
                if !seen.insert(name.clone()) {
                    out.push(Diagnostic::new("EDX-E2006", at, format!("another {what} is already named `{name}`")));
                }
            }
        };
        let out = &mut self.out;
        if let Some(d) = &doc.design {
            if let Some(b) = &d.board {
                dup(
                    out,
                    "layer",
                    b.stackup
                        .iter()
                        .enumerate()
                        .map(|(i, l)| (l.name.clone(), format!("/design/board/stackup/{i}/name")))
                        .collect(),
                );
            }
            dup(
                out,
                "package",
                d.packages
                    .iter()
                    .flatten()
                    .enumerate()
                    .map(|(i, p)| (p.id.clone(), format!("/design/packages/{i}/id")))
                    .collect(),
            );
            dup(
                out,
                "part",
                d.parts
                    .iter()
                    .flatten()
                    .enumerate()
                    .map(|(i, p)| (p.reference.clone(), format!("/design/parts/{i}/ref")))
                    .collect(),
            );
            dup(
                out,
                "net",
                d.nets
                    .iter()
                    .flatten()
                    .enumerate()
                    .map(|(i, n)| (n.name.clone(), format!("/design/nets/{i}/name")))
                    .collect(),
            );
            dup(
                out,
                "region",
                d.regions
                    .iter()
                    .enumerate()
                    .map(|(i, r)| (r.name.clone(), format!("/design/regions/{i}/name")))
                    .collect(),
            );
            dup(
                out,
                "pour",
                d.pours.iter().enumerate().map(|(i, p)| (p.name.clone(), format!("/design/pours/{i}/name"))).collect(),
            );
            dup(
                out,
                "relation",
                d.relations
                    .iter()
                    .enumerate()
                    .map(|(i, r)| (relation_id(r), format!("/design/relations/{i}")))
                    .collect(),
            );
        }
        dup(
            out,
            "constraint",
            doc.constraints
                .iter()
                .flatten()
                .enumerate()
                .map(|(i, c)| (c.id.clone(), format!("/constraints/{i}/id")))
                .collect(),
        );
    }

    // ------------------------------------------------------------ references

    fn part(&mut self, name: &str, at: String) {
        if let Some(parts) = &self.parts {
            if !parts.contains_key(name) {
                self.push("EDX-E2002", at, format!("no part `{name}`"));
            }
        }
    }

    fn pad(&mut self, p: &PadRef, at: &str) {
        let Some(parts) = &self.parts else { return };
        match parts.get(p.part.as_str()) {
            None => self.push("EDX-E2002", ptr(at, "part"), format!("no part `{}`", p.part)),
            Some(Some(pads)) if !pads.contains(p.pad.as_str()) => {
                self.push("EDX-E2003", ptr(at, "pad"), format!("part `{}` has no pad `{}`", p.part, p.pad))
            }
            Some(_) => {}
        }
    }

    fn net(&mut self, name: &str, at: String) {
        if let Some(nets) = &self.nets {
            if !nets.contains(name) {
                self.push("EDX-E2010", at, format!("no net `{name}`"));
            }
        }
    }

    fn layer(&mut self, name: &str, at: String) {
        if let Some(layers) = &self.layers {
            if !layers.contains(name) {
                self.push("EDX-E2011", at, format!("no layer `{name}` in the stack-up"));
            }
        }
    }

    fn region_refs(&mut self, regions: &[Region], base: &str) {
        for (i, r) in regions.iter().enumerate() {
            let at = format!("{base}/{i}");
            if let RegionLayers::List(layers) = &r.layers {
                for (j, l) in layers.iter().enumerate() {
                    self.layer(l, format!("{at}/layers/{j}"));
                }
            }
            for (j, n) in r.except_nets.iter().enumerate() {
                self.net(n, format!("{at}/except_nets/{j}"));
            }
            match &r.owner {
                Some(Owner::Part { part, .. }) => self.part(part, format!("{at}/owner/part")),
                Some(Owner::Block { block, .. }) if !self.blocks.contains(block) => {
                    self.push("EDX-E2012", format!("{at}/owner/block"), format!("no block `{block}`"))
                }
                _ => {}
            }
        }
    }

    fn pour_refs(&mut self, pours: &[Pour], base: &str) {
        for (i, p) in pours.iter().enumerate() {
            self.net(&p.net, format!("{base}/{i}/net"));
            for (j, l) in p.layers.iter().enumerate() {
                self.layer(l, format!("{base}/{i}/layers/{j}"));
            }
        }
    }

    fn references(&mut self) {
        let doc = self.doc;
        if let Some(d) = &doc.design {
            let packages: HashSet<&str> = d.packages.iter().flatten().map(|p| p.id.as_str()).collect();
            if d.packages.is_some() {
                for (i, p) in d.parts.iter().flatten().enumerate() {
                    if !packages.contains(p.package.as_str()) {
                        self.push(
                            "EDX-E2001",
                            format!("/design/parts/{i}/package"),
                            format!("no package `{}`", p.package),
                        );
                    }
                }
            }
            if let Some(b) = &d.board {
                for (i, l) in b.stackup.iter().enumerate() {
                    if let Some(n) = &l.plane_net {
                        self.net(n, format!("/design/board/stackup/{i}/plane_net"));
                    }
                }
            }
            let classes = doc.rules.as_ref().and_then(|r| r.classes.as_ref());
            let mut owner: HashSet<(&str, &str)> = HashSet::new();
            for (i, n) in d.nets.iter().flatten().enumerate() {
                for (j, p) in n.pads.iter().enumerate() {
                    let at = format!("/design/nets/{i}/pads/{j}");
                    self.pad(p, &at);
                    if !owner.insert((p.part.as_str(), p.pad.as_str())) {
                        self.push("EDX-E2004", at, format!("pad {} is already in another net", pad_id(p)));
                    }
                }
                if let (Some(class), Some(classes)) = (&n.class, classes) {
                    if !classes.contains_key(class) {
                        self.push(
                            "EDX-E2005",
                            format!("/design/nets/{i}/class"),
                            format!("no net class `{class}` in rules"),
                        );
                    }
                }
            }
            self.region_refs(&d.regions, "/design/regions");
            self.pour_refs(&d.pours, "/design/pours");
            for (i, r) in d.relations.iter().enumerate() {
                self.pad(&r.subject, &format!("/design/relations/{i}/subject"));
                self.pad(&r.object, &format!("/design/relations/{i}/object"));
                if let Some(id) = &r.id {
                    let derived = relation_id(r);
                    if *id != derived {
                        self.push(
                            "EDX-E2009",
                            format!("/design/relations/{i}/id"),
                            format!("the id should be `{derived}`"),
                        );
                    }
                }
            }
        }
        for (i, c) in doc.constraints.iter().flatten().enumerate() {
            for (k, op) in c.operands.iter().enumerate() {
                let at = format!("/constraints/{i}/operands/{k}");
                match op {
                    Operand::Pad(p) => self.pad(p, &at),
                    Operand::Part { part, .. } => self.part(part, ptr(&at, "part")),
                    Operand::Net { net, .. } => self.net(net, ptr(&at, "net")),
                    Operand::Path { path, .. } => {
                        for (m, p) in path.iter().enumerate() {
                            self.pad(p, &format!("{at}/path/{m}"));
                        }
                    }
                    Operand::Region { region, .. }
                        if self.regions.as_ref().is_some_and(|r| !r.contains(region.as_str())) =>
                    {
                        self.push("EDX-E2012", ptr(&at, "region"), format!("no region `{region}`"))
                    }
                    Operand::Block { block, .. } if !self.blocks.contains(block) => {
                        self.push("EDX-E2012", ptr(&at, "block"), format!("no block `{block}`"))
                    }
                    Operand::Relations { relations, .. } => {
                        for (m, id) in relations.iter().enumerate() {
                            if self.relations.as_ref().is_some_and(|r| !r.contains(id)) {
                                self.push("EDX-E2012", format!("{at}/relations/{m}"), format!("no relation `{id}`"));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        if let Some(layout) = &doc.layout {
            for (i, p) in layout.placement.iter().enumerate() {
                self.part(&p.reference, format!("/layout/placement/{i}/ref"));
            }
            for (i, t) in layout.copper.tracks.iter().enumerate() {
                self.net(&t.net, format!("/layout/copper/tracks/{i}/net"));
                self.layer(&t.layer, format!("/layout/copper/tracks/{i}/layer"));
            }
            for (i, a) in layout.copper.arcs.iter().enumerate() {
                self.net(&a.net, format!("/layout/copper/arcs/{i}/net"));
                self.layer(&a.layer, format!("/layout/copper/arcs/{i}/layer"));
            }
            for (i, v) in layout.copper.vias.iter().enumerate() {
                self.net(&v.net, format!("/layout/copper/vias/{i}/net"));
                for (j, l) in v.layers.iter().enumerate() {
                    self.layer(l, format!("/layout/copper/vias/{i}/layers/{j}"));
                }
            }
            self.region_refs(&layout.regions, "/layout/regions");
            self.pour_refs(&layout.pours, "/layout/pours");
            for (i, b) in layout.groups.blocks.iter().enumerate() {
                for (j, p) in b.parts.iter().enumerate() {
                    self.part(p, format!("/layout/groups/blocks/{i}/parts/{j}"));
                }
            }
            for (i, q) in layout.groups.quiet.iter().enumerate() {
                for (j, n) in q.nets.iter().enumerate() {
                    self.net(n, format!("/layout/groups/quiet/{i}/nets/{j}"));
                }
            }
        }
        if let Some(report) = &doc.report {
            for (i, p) in report.unplaced.iter().enumerate() {
                self.part(p, format!("/report/unplaced/{i}"));
            }
            for (i, u) in report.unrouted.iter().enumerate() {
                self.net(&u.net, format!("/report/unrouted/{i}/net"));
                for (j, p) in u.pads.iter().enumerate() {
                    self.pad(p, &format!("/report/unrouted/{i}/pads/{j}"));
                }
            }
        }
    }

    // ------------------------------------------------------------ geometry (sections 2.5, 5)

    fn shapes(&mut self, shapes: &[Shape], base: &str) {
        for (i, d) in shapes.iter().enumerate() {
            for (code, message) in shape::check(d) {
                self.push(code, format!("{base}/{i}"), message);
            }
        }
    }

    fn geometry(&mut self) {
        let doc = self.doc;
        let Some(d) = &doc.design else { return };
        if let Some(b) = &d.board {
            self.shapes(&b.outline, "/design/board/outline");
            let first = b.stackup.first().map(|l| l.name.as_str());
            let last = b.stackup.last().map(|l| l.name.as_str());
            if first != Some("top") || last != Some("bottom") {
                self.push(
                    "EDX-E3004",
                    "/design/board/stackup",
                    "the stack-up must start with a layer named `top` and end with one named `bottom`",
                );
            }
        }
        for (i, p) in d.packages.iter().flatten().enumerate() {
            let at = format!("/design/packages/{i}");
            self.shapes(&p.courtyard.top, &format!("{at}/courtyard/top"));
            if let Some(bottom) = &p.courtyard.bottom {
                self.shapes(bottom, &format!("{at}/courtyard/bottom"));
            }
            if let Some(body) = &p.body {
                self.shapes(&body.outline, &format!("{at}/body/outline"));
            }
            for (k, pad) in p.pads.iter().enumerate() {
                if let Some(outline) = &pad.outline {
                    for (code, message) in shape::check(outline) {
                        self.push(code, format!("{at}/pads/{k}/outline"), message);
                    }
                }
                if let (PadShape::Circle, Some([w, h])) = (pad.shape, pad.size) {
                    if shape::round_nm(w) != shape::round_nm(h) {
                        self.push(
                            "EDX-E3006",
                            format!("{at}/pads/{k}/size"),
                            format!("a circular pad must be as wide as it is high, not {w} by {h}"),
                        );
                    }
                }
            }
        }
        for (i, r) in d.regions.iter().enumerate() {
            self.shapes(&r.outline, &format!("/design/regions/{i}/outline"));
        }
        for (i, p) in d.pours.iter().enumerate() {
            self.shapes(&p.outline, &format!("/design/pours/{i}/outline"));
        }
        if let Some(layout) = &doc.layout {
            for (i, r) in layout.regions.iter().enumerate() {
                self.shapes(&r.outline, &format!("/layout/regions/{i}/outline"));
            }
            for (i, p) in layout.pours.iter().enumerate() {
                self.shapes(&p.outline, &format!("/layout/pours/{i}/outline"));
            }
        }
        // Pours of different nets overlapping on a layer at the same priority (section 5.6).
        let areas: Vec<MultiPolygon<f64>> = d.pours.iter().map(|p| shape::union(&p.outline)).collect();
        for j in 0..d.pours.len() {
            for i in 0..j {
                let (a, b) = (&d.pours[i], &d.pours[j]);
                let shared = a.layers.iter().any(|l| b.layers.contains(l));
                if shared && a.net != b.net && a.priority == b.priority && shape::overlap(&areas[i], &areas[j]) {
                    self.push(
                        "EDX-E3005",
                        format!("/design/pours/{j}"),
                        format!(
                            "pour `{}` overlaps pour `{}` of another net at the same priority {}",
                            b.name, a.name, b.priority
                        ),
                    );
                    break;
                }
            }
        }
    }

    // ------------------------------------------------------------ completeness (section 4)

    fn completeness(&mut self) {
        let doc = self.doc;
        if doc.layout.is_none() && doc.report.is_none() {
            return;
        }
        match &doc.design {
            None => self.push("EDX-E2008", "", "a layout or report needs a design"),
            Some(d) => {
                let missing: Vec<&str> = [
                    ("board", d.board.is_some()),
                    ("packages", d.packages.is_some()),
                    ("parts", d.parts.is_some()),
                    ("nets", d.nets.is_some()),
                ]
                .into_iter()
                .filter_map(|(name, present)| (!present).then_some(name))
                .collect();
                if !missing.is_empty() {
                    self.push(
                        "EDX-E2008",
                        "/design",
                        format!("a layout or report needs a complete design; missing: {}", missing.join(", ")),
                    );
                }
            }
        }
        match doc.rules.as_ref() {
            None => self.push("EDX-E2008", "", "a layout or report needs rules"),
            Some(rules) => match rules.classes.as_ref() {
                None => self.push("EDX-E2008", "/rules", "a layout or report needs net classes"),
                Some(classes) => match classes.get("default") {
                    None => self.push("EDX-E2008", "/rules/classes", "net class `default` is required"),
                    Some(c) if c.clearance.is_none() || c.track_width.is_none() || c.via.is_none() => self.push(
                        "EDX-E2008",
                        "/rules/classes/default",
                        "net class `default` needs clearance, track_width and via",
                    ),
                    Some(_) => {}
                },
            },
        }
        if let (Some(d), Some(rules)) = (&doc.design, &doc.rules) {
            for (i, r) in d.relations.iter().enumerate() {
                if !rules.relations.contains_key(&r.kind) && !self.ignored.contains(r.kind.as_str()) {
                    self.push(
                        "EDX-E4001",
                        format!("/design/relations/{i}/kind"),
                        format!("relation kind `{}` has no rule", r.kind),
                    );
                }
            }
        }
    }

    // ------------------------------------------------------------ result inputs (section 11)

    fn inputs(&mut self) {
        let doc = self.doc;
        let stated = [
            ("/layout/inputs", doc.layout.as_ref().and_then(|l| l.inputs.as_ref())),
            ("/report/inputs", doc.report.as_ref().and_then(|r| r.inputs.as_ref())),
        ];
        if stated.iter().all(|(_, i)| i.is_none()) {
            return;
        }
        let actual = crate::canonical::hashes(doc);
        for (at, inputs) in stated {
            let Some(inputs) = inputs else { continue };
            for (section, claimed, hash) in [
                ("design", &inputs.design, &actual.design),
                ("rules", &inputs.rules, &actual.rules),
                ("constraints", &inputs.constraints, &actual.constraints),
            ] {
                if let Some(claimed) = claimed {
                    if Some(claimed) != hash.as_ref() {
                        self.push(
                            "EDX-W2013",
                            ptr(at, section),
                            format!("made from a different `{section}` than this document has"),
                        );
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------ contradictions (section 9.5)

    fn contradictions(&mut self) {
        let mut seen: BTreeMap<(&str, &str), (usize, &Value)> = BTreeMap::new();
        let mut found = BTreeSet::new();
        for (i, c) in self.doc.constraints.iter().flatten().enumerate() {
            if c.origin != Origin::Authored || !(c.kind == "fixed" || c.kind == "side") {
                continue;
            }
            let (Some(Operand::Part { part, .. }), Some(value)) = (c.operands.first(), &c.value) else { continue };
            match seen.get(&(c.kind.as_str(), part.as_str())) {
                Some((first, earlier)) if !same_placement(earlier, value) => {
                    found.insert((i, *first, c.kind.clone(), part.clone()));
                }
                Some(_) => {}
                None => {
                    seen.insert((c.kind.as_str(), part.as_str()), (i, value));
                }
            }
        }
        for (i, first, kind, part) in found {
            self.push(
                "EDX-E4002",
                format!("/constraints/{i}"),
                format!("this `{kind}` constraint on `{part}` contradicts constraint {first}"),
            );
        }
    }
}

fn same_placement(a: &Value, b: &Value) -> bool {
    crate::merge::same(a, b)
}

/// Finds every `extensions` member's keys, with the pointer to each.
fn collect_extension_members(v: &Value, at: &str, out: &mut Vec<(String, String)>) {
    match v {
        Value::Object(map) => {
            for (k, child) in map {
                let here = ptr(at, k);
                if k == "extensions" {
                    if let Value::Object(ext) = child {
                        for name in ext.keys() {
                            out.push((name.clone(), ptr(&here, name)));
                        }
                    }
                } else if k != "extras" {
                    collect_extension_members(child, &here, out);
                }
            }
        }
        Value::Array(items) => {
            for (i, child) in items.iter().enumerate() {
                collect_extension_members(child, &ptr(at, i), out);
            }
        }
        _ => {}
    }
}
