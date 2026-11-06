//! Reads a KiCad board (`.kicad_pcb`, KiCad 6 or later) and its project's rules (`.kicad_pro`) and
//! writes them as an EDX document, with no KiCad running.
//!
//! - `design`: the board outline (Edge.Cuts) and copper stack-up, a package per footprint (pads and
//!   courtyard, as seen from the top), the parts, the nets, the author's zones as pours, and keep-out
//!   areas as regions. Each part is `fixed` or `free` as [`Fixed`] says.
//! - `rules`: the project's net classes and board minimums.
//! - `layout` (unless [`Options::layout`] is off): where the author put every part, and their tracks,
//!   arcs and vias, so a layout engine's result can be compared with the board as designed.
//!
//! What has no place in EDX (silkscreen, text, filled zone copper, teardrops) is left out, and what had
//! to be approximated is listed in [`Converted::notes`].
//!
//! ```no_run
//! let board = std::fs::read_to_string("board.kicad_pcb")?;
//! let project = std::fs::read_to_string("board.kicad_pro").ok();
//! let converted = edx_kicad::convert(&board, project.as_deref(), &edx_kicad::Options::default())?;
//! print!("{}", edx_format::to_string(&converted.document));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

pub mod geom;
mod project;
pub mod sexpr;

use std::collections::{BTreeMap, HashMap, HashSet};

use edx_format::geometry::normalize_angle;
use edx_format::model::*;
use edx_format::shape::{self, Subpath};
use edx_format::Transform;
use geo::{Area, BooleanOps, MultiPolygon, Relate};

use geom::Drawn;
pub use project::Project;
use sexpr::Node;

/// Which parts the document fixes where the author put them; the rest are `free`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Fixed {
    /// Parts the author locked, mounting holes and fiducials, and parts at the board's edge, which
    /// fit the enclosure: connectors, switches, LEDs in a panel.
    #[default]
    Edge,
    /// Only the parts the author locked.
    Locked,
    /// Every part: a routing problem only.
    All,
    /// No part.
    None,
}

#[derive(Clone, Debug)]
pub struct Options {
    pub fixed: Fixed,
    /// How close to the outline a courtyard comes to count as at the edge, mm.
    pub edge_distance: f64,
    /// Whether to write the author's placement and copper as `layout`.
    pub layout: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options { fixed: Fixed::Edge, edge_distance: 1.0, layout: true }
    }
}

/// A converted board, and what the conversion left out or approximated.
pub struct Converted {
    pub document: Document,
    pub notes: Vec<String>,
}

/// The oldest board file this reads: KiCad 6's format.
pub const OLDEST_VERSION: i64 = 20210000;

/// Converts a board file's text, with its project file's text if there is one.
pub fn convert(board: &str, project: Option<&str>, options: &Options) -> Result<Converted, String> {
    let root = sexpr::parse(board)?;
    if root.head() != Some("kicad_pcb") {
        return Err("not a KiCad board: the file does not start with (kicad_pcb".into());
    }
    let version = root.number("version").unwrap_or(0.0) as i64;
    if version < OLDEST_VERSION {
        return Err(format!(
            "the board is in KiCad 5's format (version {version}); open it in KiCad 6 or later and save it first"
        ));
    }
    let project = project.map(Project::read).transpose()?;
    Reader::new(&root, project, options)?.read()
}

/// Notes, each counted, in the order first made.
#[derive(Default)]
struct Notes(Vec<(String, usize)>);

impl Notes {
    fn add(&mut self, note: impl Into<String>) {
        let note = note.into();
        match self.0.iter_mut().find(|(n, _)| *n == note) {
            Some((_, count)) => *count += 1,
            None => self.0.push((note, 1)),
        }
    }

    fn list(self) -> Vec<String> {
        self.0.into_iter().map(|(n, c)| if c > 1 { format!("{n} ({c})") } else { n }).collect()
    }
}

/// A footprint read into package geometry.
struct Footprint {
    reference: String,
    value: Option<String>,
    lib: String,
    side: Side,
    at: Point,
    /// The EDX angle: KiCad's, plus 180 degrees on the bottom.
    angle: f64,
    locked: bool,
    dnp: bool,
    package: Package,
    /// The courtyard on the board, for the edge test.
    courtyard: Vec<String>,
    /// Each pad's net, by pad number.
    nets: Vec<(String, String)>,
}

struct Zone {
    name: Option<String>,
    net: Option<String>,
    /// KiCad's layer names.
    layers: Vec<String>,
    outline: Vec<String>,
    priority: f64,
    fill: Option<Fill>,
    connection: Option<ZoneConnection>,
    clearance: Option<f64>,
}

struct Reader<'a> {
    root: &'a Node,
    project: Option<Project>,
    options: &'a Options,
    notes: Notes,
    /// KiCad copper layer names in stack order, top first.
    copper: Vec<String>,
    /// KiCad copper layer name to EDX layer name.
    layer: HashMap<String, String>,
    /// Net numbers to names, from the table KiCad 6 to 9 write.
    net_table: HashMap<String, String>,
}

impl<'a> Reader<'a> {
    fn new(root: &'a Node, project: Option<Project>, options: &'a Options) -> Result<Self, String> {
        let mut inner: Vec<(u32, String)> = Vec::new();
        let (mut front, mut back) = (false, false);
        for l in root.child("layers").map(Node::items).unwrap_or_default().iter() {
            match l.arg(1) {
                Some("F.Cu") => front = true,
                Some("B.Cu") => back = true,
                Some(name) => {
                    if let Some(n) =
                        name.strip_prefix("In").and_then(|s| s.strip_suffix(".Cu")).and_then(|n| n.parse().ok())
                    {
                        inner.push((n, name.to_owned()));
                    }
                }
                None => {}
            }
        }
        if !(front && back) {
            return Err("the board has no F.Cu and B.Cu layers".into());
        }
        inner.sort();
        let mut copper = vec!["F.Cu".to_owned()];
        let mut layer =
            HashMap::from([("F.Cu".to_owned(), "top".to_owned()), ("B.Cu".to_owned(), "bottom".to_owned())]);
        for (n, name) in inner {
            layer.insert(name.clone(), format!("In{n}"));
            copper.push(name);
        }
        copper.push("B.Cu".into());
        let net_table =
            root.children("net").filter_map(|n| Some((n.arg(1)?.to_owned(), n.arg(2)?.to_owned()))).collect();
        Ok(Reader { root, project, options, notes: Notes::default(), copper, layer, net_table })
    }

    /// The net an item is on: `(net 3 "GND")`, `(net 3)` by the table, or `(net "GND")` from KiCad
    /// 10; a zone's `(net_name "GND")` before KiCad 10. `None` for no net.
    fn net(&self, item: &Node) -> Option<String> {
        if let Some(name) = item.value("net_name") {
            return (!name.is_empty()).then(|| name.to_owned());
        }
        let n = item.child("net")?;
        let name = match (n.items().get(1), n.arg(2)) {
            (_, Some(name)) => name.to_owned(),
            (Some(Node::Atom(number)), None) => self.net_table.get(number)?.clone(),
            (Some(Node::Str(name)), None) => name.clone(),
            _ => return None,
        };
        (!name.is_empty()).then_some(name)
    }

    /// An item's copper layers, by KiCad name: `(layer ...)` or `(layers ...)`, with `*.Cu` and
    /// `F&B.Cu` spelt out.
    fn copper_layers(&self, item: &Node) -> Vec<String> {
        let mut names: Vec<&str> =
            item.child("layers").map(|l| l.items().iter().skip(1).filter_map(Node::text).collect()).unwrap_or_default();
        names.extend(item.value("layer"));
        let mut out: Vec<String> = Vec::new();
        for n in names {
            let add: Vec<String> = match n {
                "*.Cu" => self.copper.clone(),
                "F&B.Cu" => vec!["F.Cu".into(), "B.Cu".into()],
                n if self.layer.contains_key(n) => vec![n.to_owned()],
                _ => vec![],
            };
            for a in add {
                if !out.contains(&a) {
                    out.push(a);
                }
            }
        }
        out
    }

    fn read(mut self) -> Result<Converted, String> {
        let outline = self.outline()?;
        let board_area = shape::union(&outline);
        let mut footprints = self.footprints();

        // Nets, from the pads, in the order first met.
        let mut nets: Vec<Net> = Vec::new();
        let mut net_index: HashMap<String, usize> = HashMap::new();
        for fp in &footprints {
            for (pad, net) in &fp.nets {
                let i = *net_index.entry(net.clone()).or_insert_with(|| {
                    nets.push(Net { name: net.clone(), pads: vec![], class: None, ext: Ext::default() });
                    nets.len() - 1
                });
                let pad_ref = PadRef { part: fp.reference.clone(), pad: pad.clone(), ext: Ext::default() };
                if !nets[i].pads.contains(&pad_ref) {
                    nets[i].pads.push(pad_ref);
                }
            }
        }
        let known: HashSet<String> = nets.iter().map(|n| n.name.clone()).collect();

        let rules = self.rules(&mut nets);
        let copper = self.copper_items(&known);
        let (stackup, planes) = self.stackup(&board_area, &copper, &known);
        let (pours, regions) = self.pours_and_regions(&known, &planes);

        // Packages, shared by footprints whose geometry is the same.
        let mut packages: Vec<Package> = Vec::new();
        let mut by_key: HashMap<String, String> = HashMap::new();
        let mut parts = Vec::new();
        let mut placement = Vec::new();
        for fp in &mut footprints {
            let key = package_key(&fp.package);
            let id = match by_key.get(&key) {
                Some(id) => id.clone(),
                None => {
                    let base = if fp.lib.is_empty() { fp.reference.clone() } else { fp.lib.clone() };
                    let mut id = base.clone();
                    let mut n = 1;
                    while packages.iter().any(|p| p.id == id) {
                        n += 1;
                        id = format!("{base}~{n}");
                    }
                    fp.package.id = id.clone();
                    packages.push(fp.package.clone());
                    by_key.insert(key, id.clone());
                    id
                }
            };
            let fixed = match self.options.fixed {
                Fixed::All => true,
                Fixed::None => false,
                Fixed::Locked => fp.locked,
                Fixed::Edge => {
                    fp.locked || fp.package.class == "mechanical" || self.at_edge(&fp.courtyard, &board_area, &outline)
                }
            };
            let mut attributes = BTreeMap::new();
            if !fp.lib.is_empty() {
                attributes.insert("kicad_footprint".to_owned(), fp.lib.clone());
            }
            if fp.dnp {
                attributes.insert("dnp".to_owned(), "yes".to_owned());
            }
            parts.push(Part {
                reference: fp.reference.clone(),
                package: id,
                value: fp.value.clone(),
                side: fp.side,
                placement: if fixed {
                    Placement::Fixed { at: fp.at, angle: fp.angle, ext: Ext::default() }
                } else {
                    Placement::Free { ext: Ext::default() }
                },
                attributes,
                ext: Ext::default(),
            });
            placement.push(PlacedPart {
                reference: fp.reference.clone(),
                placed: true,
                at: Some(fp.at),
                angle: Some(fp.angle),
                side: Some(fp.side),
                locked: fp.locked,
                ext: Ext::default(),
            });
        }

        let design = Design {
            board: Some(Board { outline, stackup, ext: Ext::default() }),
            packages: Some(packages),
            parts: Some(parts),
            nets: Some(nets),
            regions,
            pours,
            relations: vec![],
            ext: Ext::default(),
        };
        let mut document =
            Document { edx: "1.0".into(), design: Some(design), rules: Some(rules), ..Document::default() };
        if self.options.layout {
            let hashes = edx_format::hashes(&document);
            document.layout = Some(Layout {
                inputs: Some(Inputs {
                    design: hashes.design,
                    rules: hashes.rules,
                    constraints: None,
                    ext: Ext::default(),
                }),
                placement,
                copper,
                ..Layout::default()
            });
        }
        Ok(Converted { document, notes: self.notes.list() })
    }

    // ------------------------------------------------------------ outline

    fn outline(&mut self) -> Result<Vec<String>, String> {
        let mut open = Vec::new();
        let mut loops = Vec::new();
        let mut take = |d: Drawn| match d {
            Drawn::Open(s) => open.extend(s),
            Drawn::Closed(s) => loops.push(s),
        };
        for item in self.root.items() {
            if item.value("layer") == Some("Edge.Cuts") {
                geom::drawn(item).map(&mut take);
            }
        }
        for fp in self.root.children("footprint") {
            let kt = kicad_transform(fp);
            for item in fp.items() {
                if item.value("layer") == Some("Edge.Cuts") {
                    if let Some(d) = geom::drawn(item) {
                        take(d.map(&|p| kt.apply(p)));
                    }
                }
            }
        }
        let (chained, dropped) = geom::chain(open);
        if dropped > 0 {
            self.notes.add(format!("{dropped} Edge.Cuts pieces close no loop and are left out of the outline"));
        }
        loops.extend(chained);
        let subpaths: Vec<Subpath> = loops.iter().map(|l| geom::subpath(l)).collect();
        let mut outline = Vec::new();
        for subs in geom::nest(subpaths) {
            let d = shape::write(&subs);
            match shape::check(&d).first() {
                None => outline.push(d),
                Some((_, problem)) => self.notes.add(format!("an Edge.Cuts outline is left out: {problem}")),
            }
        }
        if outline.is_empty() {
            return Err("the board has no closed outline on Edge.Cuts".into());
        }
        if outline.len() > 1 {
            self.notes.add(format!("the outline has {} separate pieces, as a panel does", outline.len()));
        }
        Ok(outline)
    }

    fn at_edge(&self, courtyard: &[String], board: &MultiPolygon<f64>, outline: &[String]) -> bool {
        let edges: Vec<[Point; 2]> = outline
            .iter()
            .filter_map(|d| shape::parse(d).ok())
            .flatten()
            .flat_map(|s| ring_edges(&shape::flatten(&s)))
            .collect();
        courtyard.iter().filter_map(|d| shape::parse(d).ok()).any(|subs| {
            let poly = shape::to_polygon(&subs);
            if !board.relate(&poly).is_covers() {
                return true;
            }
            let points = shape::flatten(&subs[0]);
            let own = ring_edges(&points);
            let close = |p: Point, e: &[Point; 2]| point_segment(p, e) <= self.options.edge_distance;
            points.iter().any(|&p| edges.iter().any(|e| close(p, e)))
                || edges.iter().any(|e| own.iter().any(|o| close(e[0], o)))
        })
    }

    // ------------------------------------------------------------ footprints

    fn footprints(&mut self) -> Vec<Footprint> {
        let mut out: Vec<Footprint> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        let fps: Vec<&Node> = self.root.children("footprint").collect();
        for (k, fp) in fps.into_iter().enumerate() {
            let lib = fp.arg(1).unwrap_or("").to_owned();
            let side = if fp.value("layer") == Some("B.Cu") { Side::Bottom } else { Side::Top };
            let at = fp.point("at").unwrap_or([0.0, 0.0]);
            let kicad_angle = fp.child("at").and_then(|a| a.num(3)).unwrap_or(0.0);
            let angle = normalize_angle(if side == Side::Bottom { kicad_angle + 180.0 } else { kicad_angle });
            let kt = kicad_transform(fp);
            let et = Transform::new(at, angle, side);

            let property = |name: &str, field: &str| {
                fp.children("property")
                    .find(|p| p.arg(1) == Some(name))
                    .and_then(|p| p.arg(2))
                    .or_else(|| fp.children("fp_text").find(|t| t.arg(1) == Some(field)).and_then(|t| t.arg(2)))
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
            };
            let mut reference = property("Reference", "reference").unwrap_or_else(|| format!("FP{}", k + 1));
            let value = property("Value", "value");

            let mut pads = Vec::new();
            let mut nets = Vec::new();
            let mut unnamed = 0;
            for pad in fp.children("pad") {
                let number = pad_number(pad, &mut unnamed);
                if let Some(p) = self.pad(pad, number, &kt, &et) {
                    // KiCad 6 on names each unconnected pin's own net `unconnected-(...)`.
                    if let Some(net) = self.net(pad).filter(|n| !n.starts_with("unconnected-")) {
                        nets.push((p.number.clone(), net));
                    }
                    pads.push(p);
                }
            }
            if pads.is_empty() {
                self.notes.add("footprints with no pads (logos, graphics) are left out");
                continue;
            }
            if !seen.insert(reference.clone()) {
                let base = reference.clone();
                let mut n = 1;
                while seen.contains(&reference) {
                    n += 1;
                    reference = format!("{base}~{n}");
                }
                seen.insert(reference.clone());
                self.notes.add("footprints share a reference; later ones are renamed ref~2, ref~3");
            }

            // The courtyard on the part's own side, on the board, then in the package.
            let own = if side == Side::Top { "F.CrtYd" } else { "B.CrtYd" };
            let mut open = Vec::new();
            let mut loops = Vec::new();
            for item in fp.items() {
                if item.value("layer") == Some(own) {
                    match geom::drawn(item).map(|d| d.map(&|p| kt.apply(p))) {
                        Some(Drawn::Open(s)) => open.extend(s),
                        Some(Drawn::Closed(s)) => loops.push(s),
                        None => {}
                    }
                }
            }
            loops.extend(geom::chain(open).0);
            let board_loops: Vec<Subpath> = loops.iter().map(|l| geom::subpath(l)).collect();
            let mut courtyard: Vec<String> = geom::nest(et.invert_subpaths(&board_loops))
                .iter()
                .map(|subs| shape::write(subs))
                .filter(|d| shape::check(d).is_empty())
                .collect();
            if courtyard.is_empty() {
                self.notes.add("footprints with no closed courtyard get one drawn 0.25 mm round their pads");
                courtyard = vec![around_pads(&pads, 0.25)];
            }
            let on_board = courtyard.iter().filter_map(|d| et.apply_shape(d).ok()).collect();

            let attr = fp.child("attr");
            let dnp = attr.is_some_and(|a| a.has_atom("dnp")) || fp.flag("dnp");
            let class = classify(&lib, &reference, &pads);
            out.push(Footprint {
                reference,
                value,
                lib,
                side,
                at,
                angle,
                locked: fp.flag("locked"),
                dnp,
                package: Package {
                    id: String::new(),
                    class: class.into(),
                    pads,
                    courtyard: Courtyard { top: courtyard, bottom: None, ext: Ext::default() },
                    body: None,
                    ext: Ext::default(),
                },
                courtyard: on_board,
                nets,
            });
        }
        out
    }

    /// A pad in its package: KiCad gives its position in the footprint and its angle on the board;
    /// both are taken to the board and back into the package as EDX sees it.
    fn pad(&mut self, pad: &Node, number: String, kt: &Transform, et: &Transform) -> Option<Pad> {
        let kind = match pad.arg(2)? {
            "smd" | "connect" => PadKind::Smd,
            "thru_hole" => PadKind::Th,
            "np_thru_hole" => PadKind::Npth,
            other => {
                self.notes.add(format!("pads of type {other} are left out"));
                return None;
            }
        };
        let layers = self.copper_layers(pad);
        let board_layers = match kind {
            PadKind::Th | PadKind::Npth => PadLayers::Through,
            _ if layers.iter().any(|l| l == "F.Cu") => PadLayers::Top,
            _ if layers.iter().any(|l| l == "B.Cu") => PadLayers::Bottom,
            _ => {
                if !layers.is_empty() {
                    self.notes.add("surface pads on inner layers only are left out");
                }
                return None;
            }
        };
        let at = pad.child("at")?;
        let on_board = kt.apply([at.num(1)?, at.num(2)?]);
        let rotation_on_board = at.num(3).unwrap_or(0.0);
        let size = pad.child("size").and_then(|s| Some([s.num(1)?, s.num(2)?]));
        let position = et.invert(on_board);
        let rotation = et.invert_rotation(rotation_on_board);

        let mut out = Pad {
            number,
            kind,
            shape: PadShape::Rect,
            size,
            corner_radius: None,
            outline: None,
            position,
            rotation,
            layers: et.layers(board_layers),
            drill: drill(pad),
            zone_connection: zone_connection(pad),
            ext: Ext::default(),
        };
        let [w, h] = size.unwrap_or([0.0, 0.0]);
        // A polygon pad's corners, in the pad's own frame before its rotation.
        let polygon: Option<Vec<Point>> = match pad.arg(3)? {
            "rect" => None,
            "circle" => {
                out.shape = PadShape::Circle;
                out.size = Some([w, w]);
                None
            }
            "oval" => {
                out.shape = PadShape::Oval;
                None
            }
            "roundrect" => {
                let chamfer = pad.number("chamfer_ratio").unwrap_or(0.0);
                let corners = pad.child("chamfer");
                if chamfer > 0.0 && corners.is_some_and(|c| c.items().len() > 1) {
                    self.notes.add("chamfered pads become polygons, their other corners square");
                    Some(chamfered(w, h, chamfer * w.min(h), corners.unwrap()))
                } else {
                    out.shape = PadShape::Roundrect;
                    out.corner_radius = Some(pad.number("roundrect_rratio").unwrap_or(0.25).min(0.5) * w.min(h));
                    None
                }
            }
            "trapezoid" => {
                let d = pad.child("rect_delta");
                let (dx, dy) =
                    (d.and_then(|d| d.num(1)).unwrap_or(0.0) / 2.0, d.and_then(|d| d.num(2)).unwrap_or(0.0) / 2.0);
                let (hx, hy) = (w / 2.0, h / 2.0);
                Some(vec![[-hx - dy, hy + dx], [-hx + dy, -hy - dx], [hx - dy, -hy + dx], [hx + dy, hy - dx]])
            }
            "custom" => match custom(pad, w, h) {
                Some(p) => Some(p),
                None => {
                    self.notes.add("custom pads whose shape could not be read become their anchor rectangle");
                    None
                }
            },
            other => {
                self.notes.add(format!("pads of shape {other} become rectangles"));
                None
            }
        };
        if let Some(corners) = polygon {
            // On the board, then in the package, relative to the pad's position, unturned.
            let pt = Transform::new(on_board, rotation_on_board, Side::Top);
            let ring: Vec<Point> = corners
                .iter()
                .map(|&c| et.invert(pt.apply(c)))
                .map(|p| [p[0] - position[0], p[1] - position[1]])
                .collect();
            let d = shape::polygon(&ring, &[]);
            if shape::check(&d).is_empty() {
                out.shape = PadShape::Polygon;
                out.size = None;
                out.corner_radius = None;
                out.outline = Some(d);
                out.rotation = 0.0;
            } else {
                self.notes.add("polygon pads that cross themselves become their bounding rectangle");
            }
        }
        if out.size.is_none() && out.outline.is_none() {
            return None;
        }
        // Every shape but a polygon looks the same turned half a turn, and a part on the bottom
        // reaches EDX half a turn from KiCad's own library: keep the turn below 180 degrees so both
        // sides share a package.
        out.rotation = match out.shape {
            PadShape::Polygon => out.rotation,
            PadShape::Circle => 0.0,
            _ => normalize_angle(out.rotation.rem_euclid(180.0)),
        };
        out.position = out.position.map(|v| shape::round_nm(v) + 0.0);
        Some(out)
    }

    // ------------------------------------------------------------ rules

    fn rules(&mut self, nets: &mut [Net]) -> Rules {
        let project = self.project.take().unwrap_or_else(|| {
            self.notes.add("no project file: net classes are KiCad's defaults for a new project");
            let mut p = Project::default();
            p.classes.insert("Default".into(), project::KICAD_DEFAULT);
            p
        });
        let edx_name = |k: &str| if k == "Default" { "default".to_owned() } else { k.trim().to_owned() };
        let mut classes = BTreeMap::new();
        for (name, c) in &project.classes {
            let default = name == "Default";
            let pick = |v: Option<f64>, d: Option<f64>| if default { v.or(d) } else { v };
            let track = pick(c.track_width, project::KICAD_DEFAULT.track_width);
            let (pad, drill) = (
                pick(c.via_diameter, project::KICAD_DEFAULT.via_diameter),
                pick(c.via_drill, project::KICAD_DEFAULT.via_drill),
            );
            classes.insert(
                edx_name(name),
                NetClass {
                    clearance: pick(c.clearance, project::KICAD_DEFAULT.clearance),
                    track_width: track.map(|t| TrackWidth {
                        min: project.min_track_width.map_or(t, |m| m.min(t)),
                        preferred: t,
                        ext: Ext::default(),
                    }),
                    via: match (pad, drill) {
                        (Some(pad), Some(drill)) => Some(ViaSize { pad, drill, ext: Ext::default() }),
                        _ => None,
                    },
                    ..NetClass::default()
                },
            );
        }
        for net in nets.iter_mut() {
            let class = project.class_of(&net.name);
            if class != "Default" {
                net.class = Some(edx_name(class));
            }
        }
        let board = (project.copper_to_edge.is_some() || project.hole_to_hole.is_some()).then(|| BoardRules {
            copper_to_edge: project.copper_to_edge,
            hole_to_hole: project.hole_to_hole,
            ext: Ext::default(),
        });
        Rules { board, classes: Some(classes), ..Rules::default() }
    }

    // ------------------------------------------------------------ stack-up

    /// The stack-up, and the inner layers taken as planes with their nets.
    fn stackup(
        &mut self,
        board: &MultiPolygon<f64>,
        copper: &Copper,
        known: &HashSet<String>,
    ) -> (Vec<Layer>, HashMap<String, String>) {
        let physics = self.physics();
        let board_area = board.unsigned_area();
        let mut planes = HashMap::new();
        let mut layers = Vec::new();
        for (i, k) in self.copper.iter().enumerate() {
            let name = self.layer[k].clone();
            let mut layer = Layer {
                name: name.clone(),
                role: LayerRole::Signal,
                routable: true,
                plane_net: None,
                copper_thickness: None,
                dielectric: None,
                mask: None,
                ext: Ext::default(),
            };
            if let Some(p) = physics.get(k) {
                layer.copper_thickness = p.0;
                layer.dielectric = if i + 1 < self.copper.len() { p.1.clone() } else { None };
                layer.mask = if i == 0 || i + 1 == self.copper.len() { p.2.clone() } else { None };
            }
            // An inner layer half or more covered by one net's zones, with no other net's tracks, is a plane.
            if i > 0 && i + 1 < self.copper.len() {
                let mut by_net: BTreeMap<String, Vec<String>> = BTreeMap::new();
                for z in self.root.children("zone").filter(|z| z.child("keepout").is_none()) {
                    if let Some(net) = self.net(z).filter(|n| known.contains(n)) {
                        if self.copper_layers(z).contains(k) {
                            by_net.entry(net).or_default().extend(zone_outline(z));
                        }
                    }
                }
                let covered = |shapes: &Vec<String>| shape::union(shapes).intersection(board).unsigned_area();
                let best = by_net.iter().map(|(n, s)| (n, covered(s))).max_by(|a, b| a.1.total_cmp(&b.1));
                let tracks_on = |net: &str| {
                    copper.tracks.iter().any(|t| t.layer == name && t.net != net)
                        || copper.arcs.iter().any(|a| a.layer == name && a.net != net)
                };
                if let Some((net, area)) = best {
                    if board_area > 0.0 && area / board_area >= 0.5 && !tracks_on(net) {
                        layer.role = LayerRole::Plane;
                        layer.plane_net = Some(net.clone());
                        layer.routable = copper.tracks.iter().any(|t| t.layer == name);
                        planes.insert(k.clone(), net.clone());
                    }
                }
            }
            layers.push(layer);
        }
        (layers, planes)
    }

    /// Per KiCad copper layer: its thickness, the dielectric below it, and its solder mask.
    #[allow(clippy::type_complexity)]
    fn physics(&mut self) -> HashMap<String, (Option<f64>, Option<Dielectric>, Option<Mask>)> {
        let mut out: HashMap<String, (Option<f64>, Option<Dielectric>, Option<Mask>)> = HashMap::new();
        let Some(stack) = self.root.child("setup").and_then(|s| s.child("stackup")) else { return out };
        let mut above: Option<String> = None;
        let mut masks: HashMap<&str, Mask> = HashMap::new();
        for l in stack.children("layer") {
            let Some(name) = l.arg(1) else { continue };
            let kind = l.value("type").unwrap_or("");
            if self.layer.contains_key(name) {
                out.entry(name.to_owned()).or_default().0 = l.number("thickness");
                above = Some(name.to_owned());
            } else if name == "F.Mask" || name == "B.Mask" {
                if let Some(thickness) = l.number("thickness").filter(|t| *t > 0.0) {
                    let er = l.number("epsilon_r").unwrap_or_else(|| {
                        self.notes.add("solder mask with no permittivity is taken at KiCad's default, 3.3");
                        3.3
                    });
                    masks.insert(
                        if name == "F.Mask" { "F.Cu" } else { "B.Cu" },
                        Mask { thickness, er, ext: Ext::default() },
                    );
                }
            } else if kind == "core" || kind == "prepreg" || name.starts_with("dielectric") {
                let Some(above) = &above else { continue };
                // A dielectric may be built of sublayers, each after an `addsublayer`.
                let mut subs = vec![(0.0, None, None, None)];
                for n in l.items().iter().skip(2) {
                    let last = subs.last_mut().unwrap();
                    match n.head() {
                        Some("thickness") => last.0 = n.num(1).unwrap_or(0.0),
                        Some("epsilon_r") => last.1 = n.num(1),
                        Some("loss_tangent") => last.2 = n.num(1),
                        Some("material") => last.3 = n.arg(1).map(str::to_owned),
                        _ if n.text() == Some("addsublayer") => subs.push((0.0, None, None, None)),
                        _ => {}
                    }
                }
                let thickness: f64 = subs.iter().map(|s| s.0).sum();
                let weighted = |f: fn(&(f64, Option<f64>, Option<f64>, Option<String>)) -> Option<f64>| {
                    subs.iter().map(|s| f(s).map(|v| v * s.0)).sum::<Option<f64>>().map(|v| v / thickness)
                };
                let (Some(er), true) = (weighted(|s| s.1), thickness > 0.0) else { continue };
                if er <= 1.0 {
                    continue;
                }
                if subs.len() > 1 {
                    self.notes.add(
                        "dielectrics built of sublayers are taken as one, their permittivity averaged by thickness",
                    );
                }
                out.entry(above.clone()).or_default().1 = Some(Dielectric {
                    thickness,
                    er,
                    loss_tangent: weighted(|s| s.2),
                    kind: match kind {
                        "core" => Some(DielectricKind::Core),
                        "prepreg" => Some(DielectricKind::Prepreg),
                        _ => None,
                    },
                    material: subs[0].3.clone(),
                    er_frequency: None,
                    ext: Ext::default(),
                });
            }
        }
        for (layer, mask) in masks {
            out.entry(layer.to_owned()).or_default().2 = Some(mask);
        }
        out
    }

    // ------------------------------------------------------------ zones

    fn pours_and_regions(
        &mut self,
        known: &HashSet<String>,
        planes: &HashMap<String, String>,
    ) -> (Vec<Pour>, Vec<Region>) {
        let mut zones = Vec::new();
        let mut regions = Vec::new();
        let mut names: HashSet<String> = HashSet::new();
        let unique = |names: &mut HashSet<String>, base: String| {
            let mut name = base.clone();
            let mut n = 1;
            while !names.insert(name.clone()) {
                n += 1;
                name = format!("{base} {n}");
            }
            name
        };
        for (k, z) in self.root.children("zone").enumerate() {
            if z.child("attr").and_then(|a| a.child("teardrop")).is_some() {
                self.notes.add("teardrops are left out");
                continue;
            }
            let outline: Vec<String> = zone_outline(z);
            if outline.is_empty() {
                self.notes.add("zones whose outline crosses itself are left out");
                continue;
            }
            let layers = self.copper_layers(z);
            if layers.is_empty() {
                continue;
            }
            if let Some(keepout) = z.child("keepout") {
                let excludes: Vec<String> = [
                    ("tracks", "tracks"),
                    ("vias", "vias"),
                    ("pads", "pads"),
                    ("copperpour", "pours"),
                    ("footprints", "parts"),
                ]
                .iter()
                .filter(|(k, _)| keepout.value(k) == Some("not_allowed"))
                .map(|(_, e)| e.to_string())
                .collect();
                if excludes.is_empty() {
                    continue;
                }
                let base = z
                    .value("name")
                    .filter(|n| !n.trim().is_empty())
                    .map_or_else(|| format!("keepout {}", k + 1), |n| n.trim().to_owned());
                regions.push(Region {
                    name: unique(&mut names, base),
                    outline,
                    layers: if layers.len() == self.copper.len() {
                        RegionLayers::All("all".into())
                    } else {
                        RegionLayers::List(layers.iter().map(|l| self.layer[l].clone()).collect())
                    },
                    excludes,
                    except_nets: vec![],
                    owner: None,
                    ext: Ext::default(),
                });
                continue;
            }
            let net = self.net(z);
            if net.as_ref().is_some_and(|n| !known.contains(n)) {
                self.notes.add("zones on nets that reach no pad are left out");
                continue;
            }
            let fill = z.child("fill");
            let hatch = fill.is_some_and(|f| f.value("mode") == Some("hatch")).then(|| Fill::Hatch {
                width: fill.and_then(|f| f.number("hatch_thickness")).unwrap_or(0.5),
                gap: fill.and_then(|f| f.number("hatch_gap")).unwrap_or(0.5),
                angle: normalize_angle(fill.and_then(|f| f.number("hatch_orientation")).unwrap_or(0.0)),
                ext: Ext::default(),
            });
            let connect = z.child("connect_pads");
            let mode = match connect.and_then(|c| c.items().get(1)) {
                Some(Node::Atom(a)) if a == "yes" => ZoneMode::Solid,
                Some(Node::Atom(a)) if a == "no" => ZoneMode::None,
                _ => ZoneMode::Thermal,
            };
            zones.push(Zone {
                name: z.value("name").map(str::trim).filter(|n| !n.is_empty()).map(str::to_owned),
                connection: net.as_ref().map(|_| ZoneConnection {
                    mode,
                    spoke_width: fill.and_then(|f| f.number("thermal_bridge_width")),
                    gap: fill.and_then(|f| f.number("thermal_gap")),
                    ext: Ext::default(),
                }),
                // A plane's own zones are the plane: they leave its layer.
                layers: layers
                    .into_iter()
                    .filter(|l| !(planes.contains_key(l) && planes.get(l) == net.as_ref()))
                    .collect(),
                net,
                outline,
                priority: z.number("priority").unwrap_or(0.0),
                fill: hatch,
                clearance: connect.and_then(|c| c.number("clearance")),
            });
        }
        // EDX lets no two pours of different nets overlap at one priority; KiCad's ties are broken by
        // the order the zones are written in.
        let mut order: Vec<usize> = (0..zones.len()).filter(|&i| !zones[i].layers.is_empty()).collect();
        order.sort_by(|&a, &b| zones[a].priority.total_cmp(&zones[b].priority).then(a.cmp(&b)));
        let mut pours = Vec::new();
        for (rank, i) in order.into_iter().enumerate() {
            let z = &zones[i];
            let base = z.name.clone().unwrap_or_else(|| {
                format!(
                    "{} {}",
                    z.net.as_deref().unwrap_or("no net"),
                    z.layers.iter().map(|l| self.layer[l].as_str()).collect::<Vec<_>>().join("+")
                )
            });
            pours.push(Pour {
                name: unique(&mut names, base),
                net: z.net.clone(),
                layers: z.layers.iter().map(|l| self.layer[l].clone()).collect(),
                outline: z.outline.clone(),
                priority: rank as u32 + 1,
                fill: z.fill.clone(),
                connection: z.connection.clone(),
                clearance: z.clearance,
                ext: Ext::default(),
            });
        }
        (pours, regions)
    }

    // ------------------------------------------------------------ copper

    fn copper_items(&mut self, known: &HashSet<String>) -> Copper {
        let mut copper = Copper::default();
        for item in self.root.items() {
            let head = item.head();
            if !matches!(head, Some("segment" | "arc" | "via")) {
                continue;
            }
            let Some(net) = self.net(item).filter(|n| known.contains(n)) else {
                self.notes.add("tracks and vias on no net, or on a net that reaches no pad, are left out");
                continue;
            };
            let locked = item.flag("locked");
            match head {
                Some("via") => {
                    let layers = item
                        .child("layers")
                        .map(|l| l.items().iter().skip(1).filter_map(Node::text).collect::<Vec<_>>())
                        .unwrap_or_default();
                    let from = layers.first().and_then(|l| self.layer.get(*l)).cloned().unwrap_or_else(|| "top".into());
                    let to = layers.get(1).and_then(|l| self.layer.get(*l)).cloned().unwrap_or_else(|| "bottom".into());
                    let (Some(at), Some(pad), Some(drill)) =
                        (item.point("at"), item.number("size"), item.number("drill"))
                    else {
                        continue;
                    };
                    copper.vias.push(Via { net, at, pad, drill, layers: [from, to], locked, ext: Ext::default() });
                }
                _ => {
                    let Some(layer) = item.value("layer").and_then(|l| self.layer.get(l)).cloned() else { continue };
                    let width = item.number("width").unwrap_or(0.0);
                    let (Some(start), Some(end)) = (item.point("start"), item.point("end")) else { continue };
                    if head == Some("arc") {
                        let Some(mid) = item.point("mid") else { continue };
                        copper.arcs.push(Arc { net, layer, width, start, mid, end, locked, ext: Ext::default() });
                    } else if start != end {
                        copper.tracks.push(Track { net, layer, width, start, end, locked, ext: Ext::default() });
                    }
                }
            }
        }
        copper
    }
}

/// A zone's outline as shapes: each `(polygon ...)` a loop, loops inside others as their holes.
fn zone_outline(z: &Node) -> Vec<String> {
    let loops: Vec<Subpath> = z
        .children("polygon")
        .filter_map(|p| p.child("pts"))
        .map(geom::pts)
        .filter(|s| s.len() >= 3)
        .map(|s| geom::subpath(&s))
        .collect();
    geom::nest(loops).iter().map(|s| shape::write(s)).filter(|d| shape::check(d).is_empty()).collect()
}

/// How KiCad places a footprint's own items: by its position and angle, never mirrored, since a
/// footprint on the bottom has its items written already flipped.
fn kicad_transform(fp: &Node) -> Transform {
    let at = fp.point("at").unwrap_or([0.0, 0.0]);
    Transform::new(at, fp.child("at").and_then(|a| a.num(3)).unwrap_or(0.0), Side::Top)
}

/// A pad's number; pads with none (mounting holes) are numbered `~1`, `~2` in their footprint.
fn pad_number(pad: &Node, unnamed: &mut usize) -> String {
    match pad.arg(1).map(str::trim) {
        Some(n) if !n.is_empty() => n.to_owned(),
        _ => {
            *unnamed += 1;
            format!("~{unnamed}")
        }
    }
}

fn drill(pad: &Node) -> Option<Drill> {
    let d = pad.child("drill")?;
    let numbers: Vec<f64> = d
        .items()
        .iter()
        .skip(1)
        .filter_map(|n| match n {
            Node::Atom(a) => a.parse().ok(),
            _ => None,
        })
        .collect();
    let w = *numbers.first()?;
    match numbers.get(1) {
        Some(&h) if d.has_atom("oval") && h != w => Some(Drill::Slot { slot: [w, h], ext: Ext::default() }),
        _ => (w > 0.0).then(|| Drill::Round { diameter: w, ext: Ext::default() }),
    }
}

fn zone_connection(pad: &Node) -> Option<ZoneConnection> {
    let mode = match pad.number("zone_connect")? as i64 {
        0 => ZoneMode::None,
        2 => ZoneMode::Solid,
        _ => ZoneMode::Thermal,
    };
    Some(ZoneConnection {
        mode,
        spoke_width: pad.number("thermal_bridge_width").or_else(|| pad.number("thermal_width")),
        gap: pad.number("thermal_gap"),
        ext: Ext::default(),
    })
}

/// A rectangle with some corners cut at 45 degrees, `c` along each side.
fn chamfered(w: f64, h: f64, c: f64, corners: &Node) -> Vec<Point> {
    let (x, y) = (w / 2.0, h / 2.0);
    let cut = |name: &str| corners.has_atom(name);
    let mut ring = Vec::new();
    // y grows downwards: top_left is (-x, -y).
    for (corner, p, a, b) in [
        ("top_left", [-x, -y], [-x, -y + c], [-x + c, -y]),
        ("top_right", [x, -y], [x - c, -y], [x, -y + c]),
        ("bottom_right", [x, y], [x, y - c], [x - c, y]),
        ("bottom_left", [-x, y], [-x + c, y], [-x, y - c]),
    ] {
        if cut(corner) {
            ring.extend([a, b]);
        } else {
            ring.push(p);
        }
    }
    ring
}

/// A custom pad's copper: its anchor and its filled primitives, joined, in the pad's own frame.
fn custom(pad: &Node, w: f64, h: f64) -> Option<Vec<Point>> {
    let anchor = pad.child("options").and_then(|o| o.value("anchor")).unwrap_or("rect");
    let mut shapes = vec![if anchor == "circle" {
        shape::write(&[geom::subpath(&geom::circle([0.0, 0.0], w / 2.0))])
    } else {
        shape::polygon(&[[-w / 2.0, -h / 2.0], [w / 2.0, -h / 2.0], [w / 2.0, h / 2.0], [-w / 2.0, h / 2.0]], &[])
    }];
    for prim in pad.child("primitives")?.items().iter().skip(1) {
        let segs = match geom::drawn(prim) {
            Some(Drawn::Closed(s)) => s,
            _ => continue,
        };
        let d = shape::write(&[geom::subpath(&segs)]);
        if shape::check(&d).is_empty() {
            shapes.push(d);
        }
    }
    let joined = shape::union(&shapes);
    let biggest = joined.0.iter().max_by(|a, b| a.unsigned_area().total_cmp(&b.unsigned_area()))?;
    let mut ring: Vec<Point> = biggest.exterior().coords().map(|c| [c.x, c.y]).collect();
    ring.pop();
    (ring.len() >= 3).then_some(ring)
}

/// A rectangle `margin` round a package's pads, as a shape.
fn around_pads(pads: &[Pad], margin: f64) -> String {
    let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
    for p in pads {
        let r = match (p.size, &p.outline) {
            (Some([w, h]), _) => w.hypot(h) / 2.0,
            (None, Some(d)) => shape::parse(d)
                .ok()
                .map_or(0.5, |s| shape::flatten(&s[0]).iter().map(|q| q[0].hypot(q[1])).fold(0.0, f64::max)),
            _ => 0.5,
        };
        for i in 0..2 {
            lo[i] = lo[i].min(p.position[i] - r);
            hi[i] = hi[i].max(p.position[i] + r);
        }
    }
    let (l, h) = ([lo[0] - margin, lo[1] - margin], [hi[0] + margin, hi[1] + margin]);
    shape::polygon(&[l, [h[0], l[1]], h, [l[0], h[1]]], &[])
}

/// What kind of part a footprint is, from its library name, then its reference's letters.
fn classify(lib: &str, reference: &str, pads: &[Pad]) -> &'static str {
    let l = lib.to_ascii_lowercase();
    let library = l.split(':').next().unwrap_or("");
    if pads.iter().all(|p| p.kind == PadKind::Npth) || l.contains("mountinghole") || l.contains("fiducial") {
        return "mechanical";
    }
    if library.starts_with("resistor")
        || library.starts_with("capacitor")
        || library.starts_with("inductor")
        || library.starts_with("crystal")
    {
        return "passive";
    }
    if library.starts_with("connector") {
        return "connector";
    }
    let prefix: String =
        reference.chars().take_while(|c| c.is_ascii_alphabetic()).collect::<String>().to_ascii_uppercase();
    match prefix.as_str() {
        "R" | "C" | "L" | "FB" | "RN" | "RV" | "F" | "Y" => "passive",
        "U" | "IC" => "ic",
        "J" | "P" | "CN" | "CON" | "USB" => "connector",
        "H" | "MH" | "FID" | "MK" | "LOGO" | "G" => "mechanical",
        _ if library.starts_with("package_") && !library.starts_with("package_to_sot") => "ic",
        _ => "other",
    }
}

/// A package's geometry as text, to 0.1 µm, so footprints of one shape share a package.
fn package_key(p: &Package) -> String {
    fn round(v: serde_json::Value) -> serde_json::Value {
        match v {
            serde_json::Value::Number(n) => serde_json::json!((n.as_f64().unwrap_or(0.0) * 1e4).round() / 1e4 + 0.0),
            serde_json::Value::Array(a) => serde_json::Value::Array(a.into_iter().map(round).collect()),
            serde_json::Value::Object(o) => {
                serde_json::Value::Object(o.into_iter().map(|(k, v)| (k, round(v))).collect())
            }
            // Shapes in canonical form, which starts each loop at one corner and runs one way.
            serde_json::Value::String(s) => match shape::parse(&s) {
                Ok(subs) => {
                    let canonical =
                        shape::parse(&edx_format::canonical::canonical_shape(&shape::write(&subs))).unwrap_or(subs);
                    let points = canonical.iter().flat_map(shape::flatten).map(|[x, y]| format!("{x:.4},{y:.4}"));
                    serde_json::Value::String(points.collect::<Vec<_>>().join(" "))
                }
                Err(_) => serde_json::Value::String(s),
            },
            other => other,
        }
    }
    let mut p = p.clone();
    p.id.clear();
    round(serde_json::to_value(&p).unwrap_or_default()).to_string()
}

fn ring_edges(points: &[Point]) -> Vec<[Point; 2]> {
    (0..points.len()).map(|i| [points[i], points[(i + 1) % points.len()]]).collect()
}

fn point_segment(p: Point, [a, b]: &[Point; 2]) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 { 0.0 } else { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2).clamp(0.0, 1.0) };
    (p[0] - a[0] - t * dx).hypot(p[1] - a[1] - t * dy)
}
