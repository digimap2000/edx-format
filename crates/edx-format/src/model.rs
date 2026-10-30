//! The EDX document model (specification, sections 3 to 11).
//!
//! These types are filled from a document that has passed the schema, so they mirror it
//! closely. Members a newer minor version adds are ignored when reading.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

fn is_false(v: &bool) -> bool {
    !*v
}

/// A point `[x, y]` in millimetres.
pub type Point = [f64; 2];
/// A shape: SVG path data in the EDX subset (section 2.5).
pub type Shape = String;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Document {
    pub edx: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions_used: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions_required: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub design: Option<Design>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules: Option<Rules>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constraints: Option<Vec<Constraint>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<Layout>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report: Option<Report>,
    #[serde(flatten)]
    pub ext: Ext,
}

/// The `extensions` and `extras` members, which every object may carry (sections 12.3 and 12.4).
/// Readers keep both so that a document can be rewritten without loss.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
pub struct Ext {
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub extensions: Map<String, Value>,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub extras: Map<String, Value>,
}

/// A pad on the board: a part reference and a pad number (section 2.6).
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct PadRef {
    pub part: String,
    pub pad: String,
    #[serde(flatten)]
    pub ext: Ext,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Top,
    Bottom,
}

// ---------------------------------------------------------------- design

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Design {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub board: Option<Board>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub packages: Option<Vec<Package>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parts: Option<Vec<Part>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nets: Option<Vec<Net>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub regions: Vec<Region>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pours: Vec<Pour>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<Relation>,
    #[serde(flatten)]
    pub ext: Ext,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Board {
    pub outline: Vec<Shape>,
    pub stackup: Vec<Layer>,
    #[serde(flatten)]
    pub ext: Ext,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LayerRole {
    Signal,
    Plane,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Layer {
    pub name: String,
    pub role: LayerRole,
    pub routable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plane_net: Option<String>,
    #[serde(flatten)]
    pub ext: Ext,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Package {
    pub id: String,
    /// Registry: `ic`, `passive`, `connector`, `mechanical`, `other`, or an extension's name.
    pub class: String,
    pub pads: Vec<Pad>,
    pub courtyard: Courtyard,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<Body>,
    #[serde(flatten)]
    pub ext: Ext,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PadKind {
    Smd,
    Th,
    Npth,
    Edge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PadShape {
    Rect,
    Roundrect,
    Circle,
    Oval,
    Polygon,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PadLayers {
    Top,
    Bottom,
    Through,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Pad {
    pub number: String,
    pub kind: PadKind,
    pub shape: PadShape,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<[f64; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corner_radius: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline: Option<Shape>,
    pub position: Point,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub rotation: f64,
    pub layers: PadLayers,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drill: Option<Drill>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zone_connection: Option<ZoneConnection>,
    #[serde(flatten)]
    pub ext: Ext,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Drill {
    Round {
        diameter: f64,
        #[serde(flatten)]
        ext: Ext,
    },
    Slot {
        slot: [f64; 2],
        #[serde(flatten)]
        ext: Ext,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ZoneMode {
    Solid,
    Thermal,
    None,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ZoneConnection {
    pub mode: ZoneMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spoke_width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gap: Option<f64>,
    #[serde(flatten)]
    pub ext: Ext,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Courtyard {
    pub top: Vec<Shape>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bottom: Option<Vec<Shape>>,
    #[serde(flatten)]
    pub ext: Ext,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Body {
    pub outline: Vec<Shape>,
    pub height: f64,
    #[serde(flatten)]
    pub ext: Ext,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Part {
    #[serde(rename = "ref")]
    pub reference: String,
    pub package: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    pub side: Side,
    pub placement: Placement,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub attributes: BTreeMap<String, String>,
    #[serde(flatten)]
    pub ext: Ext,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Placement {
    Free {
        #[serde(flatten)]
        ext: Ext,
    },
    Fixed {
        at: Point,
        angle: f64,
        #[serde(flatten)]
        ext: Ext,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Net {
    pub name: String,
    pub pads: Vec<PadRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    #[serde(flatten)]
    pub ext: Ext,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum RegionLayers {
    /// The string `"all"`.
    All(String),
    List(Vec<String>),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Owner {
    Part {
        part: String,
        #[serde(flatten)]
        ext: Ext,
    },
    Block {
        block: String,
        #[serde(flatten)]
        ext: Ext,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Region {
    pub name: String,
    pub outline: Vec<Shape>,
    pub layers: RegionLayers,
    pub excludes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub except_nets: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<Owner>,
    #[serde(flatten)]
    pub ext: Ext,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Pour {
    pub name: String,
    pub net: String,
    pub layers: Vec<String>,
    pub outline: Vec<Shape>,
    pub priority: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<ZoneConnection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clearance: Option<f64>,
    #[serde(flatten)]
    pub ext: Ext,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Relation {
    /// Registry: `decouples`, `backs`, `filters`, `feeds`, `clocks`, `loads`, `near`, or an extension's name.
    pub kind: String,
    pub subject: PadRef,
    pub object: PadRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(flatten)]
    pub ext: Ext,
}

// ---------------------------------------------------------------- rules

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Rules {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub board: Option<BoardRules>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classes: Option<BTreeMap<String, NetClass>>,
    /// Values per relation kind; keys and values as the kind's registry entry says (section 8.3).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub relations: BTreeMap<String, Map<String, Value>>,
    #[serde(flatten)]
    pub ext: Ext,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct BoardRules {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copper_to_edge: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hole_to_hole: Option<f64>,
    #[serde(flatten)]
    pub ext: Ext,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct NetClass {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clearance: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track_width: Option<TrackWidth>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via: Option<ViaSize>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub layers: BTreeMap<String, NetClass>,
    #[serde(flatten)]
    pub ext: Ext,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TrackWidth {
    pub min: f64,
    pub preferred: f64,
    #[serde(flatten)]
    pub ext: Ext,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ViaSize {
    pub pad: f64,
    pub drill: f64,
    #[serde(flatten)]
    pub ext: Ext,
}

// ---------------------------------------------------------------- constraints

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    Derived,
    Authored,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Source {
    Derived {
        relation: String,
        rule: String,
        #[serde(flatten)]
        ext: Ext,
    },
    Authored {
        document: String,
        #[serde(flatten)]
        ext: Ext,
    },
}

/// What a constraint applies to (section 9.2).
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Operand {
    Pad(PadRef),
    Part {
        part: String,
        #[serde(flatten)]
        ext: Ext,
    },
    Net {
        net: String,
        #[serde(flatten)]
        ext: Ext,
    },
    Block {
        block: String,
        #[serde(flatten)]
        ext: Ext,
    },
    Region {
        region: String,
        #[serde(flatten)]
        ext: Ext,
    },
    Path {
        path: Vec<PadRef>,
        #[serde(flatten)]
        ext: Ext,
    },
    Relations {
        relations: Vec<String>,
        #[serde(flatten)]
        ext: Ext,
    },
    /// An operand of an extension's constraint type.
    Other(Value),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Constraint {
    pub id: String,
    /// Registry (section 9.2), or an extension's name.
    #[serde(rename = "type")]
    pub kind: String,
    pub operands: Vec<Operand>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub measure: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hard: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<f64>,
    pub origin: Origin,
    pub source: Source,
    #[serde(flatten)]
    pub ext: Ext,
}

impl Constraint {
    /// Whether the constraint must hold: `hard` if stated, else true except for a `square_on_pin`
    /// with no value (section 9.1).
    pub fn is_hard(&self) -> bool {
        self.hard.unwrap_or(!(self.kind == "square_on_pin" && self.value.is_none()))
    }
}

// ---------------------------------------------------------------- results

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Inputs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub design: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constraints: Option<String>,
    #[serde(flatten)]
    pub ext: Ext,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Layout {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inputs: Option<Inputs>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub placement: Vec<PlacedPart>,
    #[serde(default)]
    pub copper: Copper,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub regions: Vec<Region>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pours: Vec<Pour>,
    #[serde(default)]
    pub groups: Groups,
    #[serde(flatten)]
    pub ext: Ext,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PlacedPart {
    #[serde(rename = "ref")]
    pub reference: String,
    pub placed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<Point>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angle: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<Side>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub locked: bool,
    #[serde(flatten)]
    pub ext: Ext,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Copper {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tracks: Vec<Track>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arcs: Vec<Arc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vias: Vec<Via>,
    #[serde(flatten)]
    pub ext: Ext,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Track {
    pub net: String,
    pub layer: String,
    pub width: f64,
    pub start: Point,
    pub end: Point,
    #[serde(default, skip_serializing_if = "is_false")]
    pub locked: bool,
    #[serde(flatten)]
    pub ext: Ext,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Arc {
    pub net: String,
    pub layer: String,
    pub width: f64,
    pub start: Point,
    pub mid: Point,
    pub end: Point,
    #[serde(default, skip_serializing_if = "is_false")]
    pub locked: bool,
    #[serde(flatten)]
    pub ext: Ext,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Via {
    pub net: String,
    pub at: Point,
    pub pad: f64,
    pub drill: f64,
    pub layers: [String; 2],
    #[serde(default, skip_serializing_if = "is_false")]
    pub locked: bool,
    #[serde(flatten)]
    pub ext: Ext,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Groups {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<BlockGroup>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quiet: Vec<QuietGroup>,
    #[serde(flatten)]
    pub ext: Ext,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BlockGroup {
    pub name: String,
    pub parts: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub angles: Vec<f64>,
    #[serde(flatten)]
    pub ext: Ext,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct QuietGroup {
    pub name: String,
    pub nets: Vec<String>,
    pub margin: f64,
    #[serde(flatten)]
    pub ext: Ext,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Met,
    Broken,
    NotYetMeasurable,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Where {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nets: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pads: Vec<PadRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<Point>,
    #[serde(flatten)]
    pub ext: Ext,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ConstraintStatus {
    pub id: String,
    pub status: Status,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub measured: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub measure: Option<String>,
    pub hard: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<f64>,
    #[serde(default, rename = "where", skip_serializing_if = "Option::is_none")]
    pub location: Option<Where>,
    #[serde(flatten)]
    pub ext: Ext,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Unrouted {
    pub net: String,
    pub pads: Vec<PadRef>,
    #[serde(flatten)]
    pub ext: Ext,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ClearanceViolation {
    pub layer: String,
    pub at: Point,
    pub items: Vec<Where>,
    pub actual: f64,
    pub required: f64,
    #[serde(flatten)]
    pub ext: Ext,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Gates {
    pub constraints_met: bool,
    pub all_placed: bool,
    pub all_routed: bool,
    pub clearance_clean: bool,
    #[serde(flatten)]
    pub ext: Ext,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Report {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inputs: Option<Inputs>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<ConstraintStatus>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unplaced: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overlaps: Vec<[Operand; 2]>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unrouted: Vec<Unrouted>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub clearance: Vec<ClearanceViolation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gates: Option<Gates>,
    #[serde(flatten)]
    pub ext: Ext,
}
